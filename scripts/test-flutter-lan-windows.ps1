param(
    [string]$Bundle = 'target/Classroom-Console-Windows-x64',
    [ValidateRange(1024, 65524)][int]$Port = 48700,
    [int]$Seconds = 900,
    [string]$BindIp = '192.168.50.94'
)
# Bounded LAN test controller; commands are supplied through command.json. Owns only processes launched by this script.
# IPC authentication stays in memory; endpoint files must not be published.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$bundlePath = Join-Path $root $Bundle
$run = Join-Path $root ('target/windows-mac-wifi/' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Force $run | Out-Null
$endpoint = Join-Path $run 'endpoint.json'
$script:connection = $null
$script:state = $null
$script:requestId = 0
$owned = [Collections.Generic.List[Diagnostics.Process]]::new()
$results = [Collections.Generic.List[object]]::new()
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ClassroomSmokeWindow {
    public delegate bool EnumProc(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr parameter);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, System.Text.StringBuilder text, int count);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr window, uint message, IntPtr first, IntPtr second);
    [DllImport("user32.dll")] static extern bool GetGUIThreadInfo(uint thread, ref GuiInfo info);
    [StructLayout(LayoutKind.Sequential)] struct GuiInfo {
        public uint size, flags;
        public IntPtr active, focus, capture, menuOwner, moveSize, caret;
        public int left, top, right, bottom;
    }
    static IntPtr TrayWindow(int pid) {
        IntPtr found=IntPtr.Zero;
        EnumWindows((window, parameter) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            var text=new System.Text.StringBuilder(256); GetClassName(window,text,256);
            if(owner==pid && text.ToString()=="tray_icon_app") {found=window; return false;}
            return true;
        },IntPtr.Zero);
        return found;
    }
    public static bool OpenTray(int pid) {
        var window=TrayWindow(pid);
        if(window==IntPtr.Zero) throw new Exception("Test agent tray window missing");
        // tray-icon 0.21.3: native callback 6002 with WM_RBUTTONDOWN.
        return PostMessage(window,6002,IntPtr.Zero,new IntPtr(0x204));
    }
    public static bool TrayMenuOpen(int pid) {
        var window=TrayWindow(pid); if(window==IntPtr.Zero) return false;
        uint owner; uint thread=GetWindowThreadProcessId(window,out owner);
        var info=new GuiInfo(); info.size=(uint)Marshal.SizeOf(typeof(GuiInfo));
        return GetGUIThreadInfo(thread,ref info) && (info.flags & 0x14)!=0;
    }
}
'@
function Show-OwnedWindow([Diagnostics.Process]$process) {
    $process.Refresh()
    if ($process.MainWindowHandle -ne [IntPtr]::Zero) {
        $null = [ClassroomSmokeWindow]::ShowWindow($process.MainWindowHandle, 9)
        $null = [ClassroomSmokeWindow]::SetForegroundWindow($process.MainWindowHandle)
        Start-Sleep -Milliseconds 500
    }
}
function Read-Exact([int]$count) {
    $bytes = [byte[]]::new($count)
    $offset = 0
    while ($offset -lt $count) {
        $received = $script:stream.Read($bytes, $offset, $count - $offset)
        if ($received -eq 0) { throw 'IPC connection closed' }
        $offset += $received
    }
    return ,$bytes
}
function Send-Ipc($value) {
    $bytes = [Text.Encoding]::UTF8.GetBytes(($value | ConvertTo-Json -Depth 12 -Compress))
    $header = [BitConverter]::GetBytes([uint32]$bytes.Length)
    [Array]::Reverse($header)
    $script:stream.Write($header, 0, 4)
    $script:stream.Write($bytes, 0, $bytes.Length)
}
function Read-Ipc {
    $header = Read-Exact 4
    [Array]::Reverse($header)
    $length = [BitConverter]::ToUInt32($header, 0)
    if ($length -eq 0 -or $length -gt 8388608) { throw 'Invalid IPC frame' }
    $value = [Text.Encoding]::UTF8.GetString((Read-Exact $length)) | ConvertFrom-Json
    if ($value.type -eq 'state') { $script:state = $value }
    return $value
}
function Publish-State {
    # Readers may hold state.json briefly on Windows. Publish a complete sibling
    # atomically and skip this sample on contention; never interrupt live IPC.
    $next = Join-Path $run 'state.next.json'
    $path = Join-Path $run 'state.json'
    [IO.File]::WriteAllText($next, ($script:state | ConvertTo-Json -Depth 20))
    try {
        if ([IO.File]::Exists($path)) { [IO.File]::Replace($next, $path, (Join-Path $run 'state.previous.json')) }
        else { [IO.File]::Move($next, $path) }
    } catch [IO.IOException] {
        Remove-Item -LiteralPath $next -ErrorAction SilentlyContinue
    }
}
function Command-Ipc([string]$action, [string[]]$selected = @()) {
    $script:requestId++
    Send-Ipc @{ id = $script:requestId; action = $action; selected = $selected }
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $reply = Read-Ipc
        if ($reply.id -eq $script:requestId) {
            if (!$reply.ok) { throw "Command $action failed: $($reply.error)" }
            return
        }
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "No acknowledgement: $action"
}
function Screenshot([string]$name) {
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    $bounds = [Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bitmap = [Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($bounds.Location, [Drawing.Point]::Empty, $bounds.Size)
        $bitmap.Save((Join-Path $run "$name.png"), [Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
}

Write-Output "RUN_DIR=$run"
try {
    $console = Start-Process (Join-Path $bundlePath 'classroom_console.exe') -ArgumentList @('--ipc-file', $endpoint, '--bind-ip', $BindIp, '--control-port', $Port) -WindowStyle Normal -PassThru
    $owned.Add($console)
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while (!(Test-Path -LiteralPath $endpoint)) {
        if ($console.HasExited) { throw 'Teacher console exited' }
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Agent startup timeout' }
        Start-Sleep -Milliseconds 100
    }
    $descriptor = Get-Content -LiteralPath $endpoint -Raw | ConvertFrom-Json
    $script:connection = [Net.Sockets.TcpClient]::new('127.0.0.1', $descriptor.port)
    $script:stream = $script:connection.GetStream()
    $script:stream.ReadTimeout = 25000
    Send-Ipc @{ type='hello'; protocol=1; token=$descriptor.token; ui_role='observe' }
    $null = Read-Ipc
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    $commandPath = Join-Path $run 'command.json'
    while ([DateTime]::UtcNow -lt $deadline) {
        if ($script:stream.DataAvailable) {
            $null = Read-Ipc
            if ($script:state) {
                $script:state | Add-Member -Force -NotePropertyName tray_menu_open -NotePropertyValue ([ClassroomSmokeWindow]::TrayMenuOpen($descriptor.pid))
                Publish-State
            }
        }
        if (Test-Path -LiteralPath $commandPath) {
            $command = Get-Content -LiteralPath $commandPath -Raw | ConvertFrom-Json
            Remove-Item -LiteralPath $commandPath
            if ($command.action -eq 'screenshot') { Show-OwnedWindow $console; Screenshot $command.name }
            elseif ($command.action -eq 'open_tray_menu') {
                if (![ClassroomSmokeWindow]::OpenTray($descriptor.pid)) { throw 'Tray callback failed' }
            }
            elseif ($command.action -eq 'open_observe_window') {
                $observe = Start-Process (Join-Path $bundlePath 'classroom_console.exe') -ArgumentList @('--observe-id', $command.selected[0], '--observe-name', 'Mac-WiFi-Student', '--ipc-file', $endpoint) -WindowStyle Normal -PassThru
                $owned.Add($observe)
            } else {
                $script:requestId++
                $command | Add-Member -Force -NotePropertyName id -NotePropertyValue $script:requestId
                Send-Ipc $command
                do { $reply = Read-Ipc } while ($reply.id -ne $script:requestId)
                $results.Add(@{action=$command.action;reply=$reply;time=[DateTime]::UtcNow.ToString('o')})
                Write-Output ("ACK " + ($reply | ConvertTo-Json -Compress))
            }
            if ($command.action -eq 'quit') { break }
        }
        Start-Sleep -Milliseconds 50
    }
} catch {
    $results.Add(@{action='failure';error=$_.Exception.Message})
    throw
} finally {
    if ($script:connection) { try { Command-Ipc 'quit' } catch { }; $script:connection.Dispose() }
    if ($descriptor) {
        $agent = Get-Process -Id $descriptor.pid -ErrorAction SilentlyContinue
        if ($agent) { $null = $agent.WaitForExit(3000) }
    }
    foreach ($process in $owned) { if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() } }
    # The console deliberately detaches its agent. On a failed IPC connection,
    # clean up only processes carrying this run's unique endpoint argument.
    $remaining = Get-CimInstance Win32_Process | Where-Object {
        $_.Name -match '^ClassroomAgent.exe$|^classroom_console.exe$' -and
        $_.CommandLine -and $_.CommandLine.Contains($endpoint)
    }
    foreach ($process in $remaining) {
        Stop-Process -Id $process.ProcessId -ErrorAction SilentlyContinue
    }
    $uiLogs = Join-Path $env:APPDATA 'com.classroom/classroom_console'
    foreach ($name in @('teacher-log.txt','ui.log.txt','observe-ui.log.txt')) {
        $source=Join-Path $uiLogs $name
        if (Test-Path -LiteralPath $source) { Copy-Item -LiteralPath $source -Destination (Join-Path $run $name) }
    }
    $results | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 (Join-Path $run 'results.json')
}
