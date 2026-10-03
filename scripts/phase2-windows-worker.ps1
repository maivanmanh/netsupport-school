param([string]$Root,[string]$Scenario,[ValidateSet('teacher','student')][string]$Role,
      [string]$TeacherIp,[int]$Seconds=65,[int]$Students=1,[int]$Cycles=2,[int]$Thumbnails=2,
      [switch]$RestartStudent,[switch]$ProbeScreen,[switch]$Selective,[switch]$Desktop,[int]$ControlPort=47800,[int]$MediaPort=47810,[int]$FeedbackPort=47811)
$ErrorActionPreference='Stop'
$dir=Join-Path $Root $Scenario
New-Item -ItemType Directory -Force $dir | Out-Null
$profile=Join-Path $dir 'profile.toml'
$result=@{error=$null;bootstrap_exit=$null;exit=$null;role=$Role}
$child=$null
$probe=$null
$network=@('--control-port',$ControlPort,'--media-port',$MediaPort,'--feedback-port',$FeedbackPort)
try {
    if ($ProbeScreen -and $Role -eq 'student') {
        @('CLASSROOM PHASE 2 - WINDOWS STUDENT','This is the actual student screen.','Thumbnail capture must show this Notepad window.') | Set-Content "$dir\screen-probe.txt"
        $probe=Start-Process notepad.exe -ArgumentList "$dir\screen-probe.txt" -PassThru
    }
    $bootstrap=Start-Process "$Root\Classroom.exe" -ArgumentList ($network+@('--role',$Role,'--teacher-ip',$TeacherIp,'--remember','--launch-profile',$profile,'--diagnose','--log-file',"$dir\bootstrap.log")) -PassThru -Wait
    $result.bootstrap_exit=$bootstrap.ExitCode
    if ($bootstrap.ExitCode -ne 0) { throw 'Profile bootstrap failed' }
    # Deliberately no --role and no Start click: the persisted profile chooses it.
    $a=$network+@('--launch-profile',$profile,'--log-file',"$dir\$Role.log")
    if ($Role -eq 'teacher') {
        $a+=@('--teacher-ui','--bind-ip',$TeacherIp,'--teacher-test-cycles',$Cycles,'--teacher-test-min-students',$Students,'--teacher-test-min-thumbnails',$Thumbnails,'--teacher-test-output',"$dir\ui")
        if ($Selective) { $a+=@('--teacher-test-selective') }
        if ($Desktop) { $a+=@('--teacher-test-desktop') }
    } else { $a+=@('--share-seconds',$Seconds) }
    $child=Start-Process "$Root\Classroom.exe" -ArgumentList $a -RedirectStandardOutput "$dir\$Role.stdout.log" -RedirectStandardError "$dir\$Role.stderr.log" -PassThru
    $null=$child.Handle
    if ($RestartStudent -and $Role -eq 'student') {
        Start-Sleep -Seconds 9
        if ($child.HasExited) { throw 'Student exited before restart probe' }
        Stop-Process -Id $child.Id -Force
        $child.WaitForExit()
        Copy-Item "$dir\student.log" "$dir\student-before-restart.log" -Force
        $result.restart_performed=$true
        $child=Start-Process "$Root\Classroom.exe" -ArgumentList $a -RedirectStandardOutput "$dir\student-restart.stdout.log" -RedirectStandardError "$dir\student-restart.stderr.log" -PassThru
        $null=$child.Handle
    }
    if (-not $child.WaitForExit(($Seconds+100)*1000)) { throw 'Phase 2 child deadline' }
    $result.exit=$child.ExitCode
    if ($child.ExitCode -ne 0) { throw 'Classroom exited with error' }
} catch { $result.error=$_.Exception.Message }
finally {
    if ($probe -and -not $probe.HasExited) { Stop-Process -Id $probe.Id -ErrorAction SilentlyContinue }
    if ($child -and -not $child.HasExited) { Stop-Process -Id $child.Id -Force -ErrorAction SilentlyContinue }
    $result | ConvertTo-Json -Depth 4 | Set-Content "$dir\result.json"
}
