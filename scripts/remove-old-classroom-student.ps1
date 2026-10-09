# Owner-authorized migration before a Windows Student deployment.
# Remove only a verified old installation; preserve settings, profiles and logs.
[CmdletBinding(SupportsShouldProcess=$true)]
param(
    [string]$OldInstallPath = "$env:ProgramFiles\Classroom",
    [Parameter(Mandatory=$true)][string]$ReplacementExecutable,
    [Parameter(Mandatory=$true)][string]$ReplacementSha256
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
function FullPath([string]$value) { [IO.Path]::GetFullPath($value).TrimEnd('\') }
$old = FullPath $OldInstallPath
$replacement = FullPath $ReplacementExecutable
if (!(Test-Path -LiteralPath $replacement -PathType Leaf)) { throw 'Replacement Student must be staged before removing the old installation.' }
if ((Get-FileHash -LiteralPath $replacement -Algorithm SHA256).Hash -ne $ReplacementSha256) { throw 'Replacement checksum mismatch; old installation was not touched.' }
if ((FullPath (Split-Path $replacement)) -eq $old) { throw 'Use an isolated staging path before removing old binaries.' }
if (!(Test-Path -LiteralPath $old)) { @{removed=$false; reason='Old installation absent'} | ConvertTo-Json; return }
$student = Join-Path $old 'ClassroomStudent.exe'
$guard = Join-Path $old 'ClassroomGuardService.exe'
if (!(Test-Path -LiteralPath $student) -and !(Test-Path -LiteralPath $guard)) {
    @{removed=$false; reason='Old binaries already absent'; preserved_data=$old} | ConvertTo-Json; return
}
if (!(Test-Path -LiteralPath $student)) { throw 'Old folder is not a recognized Classroom Student installation.' }
$service = Get-CimInstance Win32_Service -Filter "Name='ClassroomGuardService'"
if ($service -and (FullPath $service.PathName.Trim('"')) -ne (FullPath $guard)) {
    # This folder may be an older portable Student while another verified
    # installation owns the service. Leave that supervisor to its own migration.
    $service = $null
}
$owned = @((FullPath $student), (FullPath $guard))
$tasks = @(Get-ScheduledTask | Where-Object {
    $actions = @($_.Actions)
    $matching = @($actions | Where-Object { $_.Execute -and $owned -contains (FullPath $_.Execute.Trim('"')) })
    if ($matching.Count -gt 0 -and $matching.Count -ne $actions.Count) { throw "Mixed task actions: $($_.TaskName); refusing migration." }
    $matching.Count -gt 0
})
if (!$PSCmdlet.ShouldProcess($old, 'Stop/remove old Classroom Guard, launch tasks and binaries')) { return }
# Keep provenance and launch configuration; no settings/profile/log deletion.
$archive = Join-Path $old ('migration-record-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $archive | Out-Null
@{old_install=$old; replacement=$replacement; replacement_sha256=$ReplacementSha256; old_student_sha256=(Get-FileHash -LiteralPath $student).Hash; tasks=@($tasks | ForEach-Object {$_.TaskPath+$_.TaskName}); service=$service.PathName} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $archive 'removal.json')
$arguments = Join-Path $old 'student-launch-args.txt'
if (Test-Path -LiteralPath $arguments) { Copy-Item -LiteralPath $arguments -Destination $archive }
# Supervisor first: otherwise killing Student just causes a relaunch.
if ($service) {
    Stop-Service -Name ClassroomGuardService -Force
    $until = (Get-Date).AddSeconds(30)
    while ((Get-Service ClassroomGuardService).Status -ne 'Stopped') {
        if ((Get-Date) -gt $until) { throw 'Guard did not stop; old binaries preserved.' }
        Start-Sleep -Milliseconds 100
    }
}
foreach ($task in $tasks) {
    Stop-ScheduledTask -TaskName $task.TaskName -TaskPath $task.TaskPath
    Unregister-ScheduledTask -TaskName $task.TaskName -TaskPath $task.TaskPath -Confirm:$false
}
if ($service) {
    & sc.exe delete ClassroomGuardService | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Cannot delete Guard service; old binaries preserved.' }
}
Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and $owned -contains (FullPath $_.ExecutablePath) } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
# Only startup entries that execute the exact old Student/Guard.
foreach ($key in @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Run', 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Run')) {
    if (Test-Path $key) {
        $properties = (Get-ItemProperty $key).PSObject.Properties | Where-Object {$_.Name -notmatch '^PS'}
        foreach ($property in $properties) {
            $command = [string]$property.Value
            if ($command -match '^"([^"]+)"' -or $command -match '^([^ ]+\.exe)(?: |$)') {
                if ($owned -contains (FullPath $Matches[1])) { Remove-ItemProperty -Path $key -Name $property.Name }
            }
        }
    }
}
Get-NetFirewallApplicationFilter | Where-Object { $_.Program -and $_.Program -ne 'Any' -and $owned -contains $_.Program } | Get-NetFirewallRule | Remove-NetFirewallRule
foreach ($path in @($student, $guard, $arguments)) { if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force } }
$remaining = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and $owned -contains (FullPath $_.ExecutablePath) })
$remainingService = Get-CimInstance Win32_Service -Filter "Name='ClassroomGuardService'"
if ($remaining.Count -ne 0 -or ($remainingService -and (FullPath $remainingService.PathName.Trim('"')) -eq (FullPath $guard))) { throw 'Old processes/service still present; inspect migration before launching replacement.' }
@{removed=$true; old_install=$old; preserved_data=$old; record=$archive; old_processes=0; old_service=$false} | ConvertTo-Json
