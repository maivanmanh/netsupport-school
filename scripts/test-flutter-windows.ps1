param(
    [string]$Bundle = 'target/Classroom-Console-Windows-x64',
    [int]$Port = 48700,
    [int]$Seconds = 20
)
# Bounded, visible same-host smoke. Owns only processes launched by this script.
# IPC authentication stays in memory; endpoint files must not be published.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$bundlePath = Join-Path $root $Bundle
# Flutter stores logs/settings in a shared application-data directory even when
# IPC and control ports differ. A second console would rotate the live Teacher's
# log, invalidating its diagnostics. Require an isolated desktop/user session.
$existing = @(Get-CimInstance Win32_Process | Where-Object {
    $_.Name -eq 'classroom_console.exe' -or
    ($_.Name -eq 'ClassroomAgent.exe' -and $_.CommandLine -match '--teacher-agent')
})
if ($existing.Count) {
    throw 'A Classroom Teacher is already running. Run this smoke in an isolated Windows user session or after stopping Teacher; shared application logs/settings must not be overwritten.'
}
$run = Join-Path $root ('target/windows-flutter-smoke/' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
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
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
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
function Wait-State([scriptblock]$predicate, [int]$timeout = 20) {
    $deadline = [DateTime]::UtcNow.AddSeconds($timeout)
    do {
        if ($script:state -and (& $predicate $script:state)) { return }
        $null = Read-Ipc
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "State timeout: $predicate"
}
function Drain-For([int]$duration) {
    $deadline = [DateTime]::UtcNow.AddSeconds($duration)
    while ([DateTime]::UtcNow -lt $deadline) { $null = Read-Ipc }
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
function Launch-Student {
    $arguments = @('--teacher-ip', '127.0.0.1', '--loopback', '--control-port', $Port,
        '--media-port', ($Port + 10), '--feedback-port', ($Port + 11),
        '--launch-profile', (Join-Path $run 'student-profile.toml'),
        '--log-file', (Join-Path $run 'student.log'), '--share-seconds', '180')
    $process = Start-Process (Join-Path $root 'target/x86_64-pc-windows-msvc/release/ClassroomStudent.exe') -ArgumentList $arguments -WindowStyle Hidden -PassThru
    $owned.Add($process)
    return $process
}
Write-Output "RUN_DIR=$run"
try {
    $console = Start-Process (Join-Path $bundlePath 'classroom_console.exe') -ArgumentList @('--ipc-file', $endpoint, '--bind-ip', '127.0.0.1', '--control-port', $Port) -WindowStyle Normal -PassThru
    $owned.Add($console)
    $deadline = [DateTime]::UtcNow.AddSeconds(25)
    while (!(Test-Path -LiteralPath $endpoint)) {
        if ($console.HasExited) { throw 'Teacher console exited before agent startup' }
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Teacher agent endpoint startup timeout' }
        Start-Sleep -Milliseconds 100
    }
    $descriptor = Get-Content -LiteralPath $endpoint -Raw | ConvertFrom-Json
    $script:connection = [Net.Sockets.TcpClient]::new('127.0.0.1', $descriptor.port)
    $script:stream = $script:connection.GetStream()
    $script:stream.ReadTimeout = 25000
    Send-Ipc @{ type = 'hello'; protocol = 1; token = $descriptor.token; ui_role = 'observe' }
    $null = Read-Ipc
    $student = Launch-Student
    Wait-State { param($s) @($s.students | Where-Object online).Count -eq 1 }
    $id = @($script:state.students | Where-Object online)[0].id
    Wait-State { param($s) @($s.students | Where-Object { $_.thumbnail }).Count -gt 0 }
    Show-OwnedWindow $console
    Screenshot 'idle-roster'
    $results.Add(@{ scenario = 'registration-thumbnail'; pass = $true })
    for ($cycle = 1; $cycle -le 2; $cycle++) {
        Command-Ipc 'share_class'
        Wait-State { param($s) $s.phase -eq 'sharing' }
        Drain-For $Seconds
        Screenshot "sharing-$cycle"
        Command-Ipc 'stop'
        Wait-State { param($s) $s.phase -eq 'idle' }
        $log = Get-Content (Join-Path $run 'student.log') -Raw
        if (($log | Select-String 'student first presented frame' -AllMatches).Matches.Count -lt $cycle) { throw "No presented frame for cycle $cycle" }
        $results.Add(@{ scenario = "share-stop-$cycle"; pass = $true })
        Write-Output "PASS share-stop-$cycle"
    }
    # Same stable profile must re-register without a duplicate online row.
    $student.Kill(); $student.WaitForExit()
    Wait-State { param($s) @($s.students | Where-Object online).Count -eq 0 }
    $student = Launch-Student
    Wait-State { param($s) @($s.students | Where-Object online).Count -eq 1 }
    if (@($script:state.students | Where-Object online)[0].id -ne $id) { throw 'Student identity changed after reconnect' }
    $results.Add(@{ scenario = 'student-reconnect'; pass = $true })
    # Open the actual independent Flutter Observe window (no remote input).
    $observe = Start-Process (Join-Path $bundlePath 'classroom_console.exe') -ArgumentList @('--observe-id', $id, '--observe-name', 'Local-Windows-Student', '--ipc-file', $endpoint) -WindowStyle Normal -PassThru
    $owned.Add($observe)
    Drain-For 2
    Show-OwnedWindow $observe
    Drain-For 13
    Show-OwnedWindow $observe
    Screenshot 'observe'
    $uiLogs = Join-Path $env:APPDATA 'com.classroom/classroom_console'
    $observeLog = Get-Content (Join-Path $uiLogs 'observe-ui.log.txt') -Raw
    $recentLog = ($observeLog -split 'Observe state phase=none surface=false error=')[-1]
    $nativeFrame = $recentLog.Contains('Observe first native frame detected')
    $nativeUpdates = [regex]::Matches($recentLog, 'sequence=(\d+) source_sequence=') | ForEach-Object { [int64]$_.Groups[1].Value }
    $liveTexture = @($nativeUpdates | Where-Object { $_ -gt 5 }).Count -gt 0
    $results.Add(@{ scenario = 'observe'; pass = ($script:state.observe.phase -eq 'live' -and $nativeFrame -and $liveTexture); phase = $script:state.observe.phase; error = $script:state.observe.error; native_frame_detected = $nativeFrame; native_sequence_samples = @($nativeUpdates) })
    if ($script:state.observe.phase -ne 'live' -or !$nativeFrame -or !$liveTexture) { throw 'Observe did not produce continuously updated native Flutter frames' }
    Command-Ipc 'observe_close' @($id)
    Command-Ipc 'quit'
} catch {
    $results.Add(@{ scenario = 'failure'; pass = $false; error = $_.Exception.Message })
    Write-Output "FAIL $($_.Exception.Message)"
    throw
} finally {
    if ($script:connection) {
        try { Command-Ipc 'quit' } catch { }
        $script:connection.Dispose()
    }
    foreach ($process in $owned) {
        if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
    }
    $uiLogs = Join-Path $env:APPDATA 'com.classroom/classroom_console'
    foreach ($name in @('teacher-log.txt', 'ui.log.txt', 'observe-ui.log.txt')) {
        $source = Join-Path $uiLogs $name
        if (Test-Path -LiteralPath $source) { Copy-Item -LiteralPath $source -Destination (Join-Path $run $name) }
    }
    $results | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 (Join-Path $run 'results.json')
    Write-Output "RESULTS=$run/results.json"
}
