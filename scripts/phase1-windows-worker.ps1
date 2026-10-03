param([string]$Root, [string]$Scenario, [string]$Role, [string]$TeacherIp,
      [int]$Seconds = 35, [int]$DropEvery = 0, [int]$Count = 1,
      [switch]$CpuConversion, [switch]$Pressure,
      [int]$DisplayWidth=0, [int]$DisplayHeight=0, [switch]$UiAudit,
      [int]$DecoderThreads=0, [switch]$Gdi, [switch]$LiveMode, [int]$MinStudents=0)
$ErrorActionPreference = 'Stop'
$dir = Join-Path $Root $Scenario
New-Item -ItemType Directory -Force $dir | Out-Null
$children = @()
$workload = $null
$originalDisplay = $null
$result = @{ scenario=$Scenario; exits=@(); error=$null }
try {
    if ($Role -eq 'inventory') {
        & "$Root\phase1-display-modes.ps1" | Set-Content "$dir\display-modes.json"
        $result.exits = @(0)
        return
    }
    if ($DisplayWidth -gt 0) {
        $originalDisplay = (& "$Root\phase1-display-modes.ps1" -Width $DisplayWidth -Height $DisplayHeight | Out-String | ConvertFrom-Json)
        $originalDisplay | ConvertTo-Json | Set-Content "$dir\display-change.json"
        Start-Sleep -Seconds 2
    }
    if ($CpuConversion) { $env:CLASSROOM_CPU_CONVERSION='1' }
    if ($DecoderThreads -gt 0) { $env:CLASSROOM_MF_DECODER_THREADS=$DecoderThreads.ToString() }
    if ($Gdi) { $env:CLASSROOM_GDI_RENDERER='1' }
    $config = Join-Path $Root 'default.toml'
    if ($Pressure) {
        $config = Join-Path $dir 'pressure.toml'
        (Get-Content (Join-Path $Root 'default.toml') -Raw).Replace('adaptive_pressure_encode_ms = 28', 'adaptive_pressure_encode_ms = 1') | Set-Content $config
    }
    if ($Role -in @('teacher','teacher-room','capture','local','benchmark')) {
        $workload = Start-Process powershell -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',"$Root\phase1-windows-workload.ps1",'-Seconds',($Seconds+15)) -PassThru
        Start-Sleep -Seconds 2
    }
    for ($i=0; $i -lt $Count; $i++) {
        $name = if ($Role -eq 'student') { "student-$i" } else { 'teacher' }
        $args = @('--log-file',"$dir\$name.log",'--config',$config)
        switch ($Role) {
            'capture' { $args += @('--capture-benchmark-seconds',$Seconds) }
            'local' { $args += @('--local-media-benchmark-seconds',$Seconds) }
            'benchmark' { $args += @('--benchmark-students',3,'--benchmark-seconds',$Seconds,'--benchmark-output',"$dir\benchmark") }
            'teacher-room' { $args += @('--role','teacher','--teacher-ui','--teacher-test-cycles',3,'--teacher-test-min-students',$MinStudents,'--bind-ip',$TeacherIp) }
            'teacher' { $args += @('--role','teacher','--bind-ip',$TeacherIp,'--share-seconds',$Seconds,'--drop-every-nth-rtp-packet',$DropEvery) }
            'student' {
                $args += @('--role','student','--teacher-ip',$TeacherIp,'--share-seconds',$Seconds,'--media-port',(47810+2*$i),'--feedback-port',(47811+2*$i))
                if ($Count -gt 1) { $args += '--headless' }
            }
            default { throw "Unknown role $Role" }
        }
        $child = Start-Process "$Root\Classroom.exe" -ArgumentList $args -RedirectStandardOutput "$dir\$name.stdout.log" -RedirectStandardError "$dir\$name.stderr.log" -PassThru
        # Retain the native process handle before exit; PowerShell 5 otherwise
        # sometimes loses ExitCode when querying a reaped Start-Process result.
        $null = $child.Handle
        $children += $child
    }
    if ($Scenario -match '-(teachercrash|studentcrash)$' -and $Role -eq $(if ($Matches[1] -eq 'teachercrash') { 'teacher' } else { 'student' })) {
        $child = $children[0]
        $marker = if ($Role -eq 'teacher') { 'teacher established Student UDP media route and requested IDR' } else { 'student first presented frame' }
        $deadline = (Get-Date).AddSeconds(25)
        while (-not (Test-Path "$dir\$name.log") -or -not (Select-String -Path "$dir\$name.log" -Pattern $marker -Quiet)) {
            if ((Get-Date) -gt $deadline) { throw 'No media before crash injection' }
            Start-Sleep -Milliseconds 200
        }
        Start-Sleep -Seconds 2
        Stop-Process -Id $child.Id -Force
        $child.WaitForExit()
        $result.forced_exit = $child.ExitCode
        $children = @()
        Start-Sleep -Seconds 2
        $args[1] = "$dir\$name-restart.log"
        $child = Start-Process "$Root\Classroom.exe" -ArgumentList $args -RedirectStandardOutput "$dir\$name-restart.stdout.log" -RedirectStandardError "$dir\$name-restart.stderr.log" -PassThru
        $null = $child.Handle
        $children += $child
    }
    foreach ($child in $children) {
        if ($LiveMode) {
            Start-Sleep -Seconds 10
            $originalDisplay = (& "$Root\phase1-display-modes.ps1" -Width 3840 -Height 2160 | Out-String | ConvertFrom-Json)
            $originalDisplay | ConvertTo-Json | Set-Content "$dir\display-change.json"
            Start-Sleep -Seconds 8
            & "$Root\phase1-display-modes.ps1" -Width $originalDisplay.previousWidth -Height $originalDisplay.previousHeight | Set-Content "$dir\display-restored.json"
            $originalDisplay = $null
        }
        if ($UiAudit) {
            & "$Root\phase1-window-audit.ps1" -ProcessId $child.Id -Output "$dir\window-audit.json"
        }
        if (-not $child.WaitForExit(($Seconds+60)*1000)) { throw 'Classroom timeout' }
        $result.exits += $child.ExitCode
    }
} catch { $result.error = $_.Exception.Message }
finally {
    foreach ($child in $children) { if (-not $child.HasExited) { Stop-Process -Id $child.Id -ErrorAction SilentlyContinue } }
    if ($workload -and -not $workload.HasExited) { Stop-Process -Id $workload.Id -ErrorAction SilentlyContinue }
    if ($originalDisplay) {
        try { & "$Root\phase1-display-modes.ps1" -Width $originalDisplay.previousWidth -Height $originalDisplay.previousHeight | Set-Content "$dir\display-restored.json" }
        catch { $result.error = "Display restoration failed: $($_.Exception.Message)" }
    }
    $result | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $dir 'result.json')
}
