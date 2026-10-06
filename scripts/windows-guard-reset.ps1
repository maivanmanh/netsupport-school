# Emergency reset of everything the Classroom Guard installs on this computer.
# Run from an elevated prompt: powershell -ExecutionPolicy Bypass -File windows-guard-reset.ps1 (safe to repeat).
$ErrorActionPreference = 'Continue'
# DNS redirect (Name Resolution Policy Table rule created by the Guard).
Get-DnsClientNrptRule | Where-Object { $_.Comment -eq 'ClassroomGuard' } | Remove-DnsClientNrptRule -Force
Clear-DnsClientCache
# Windows Filtering Platform filters are dynamic and disappear when ClassroomStudent.exe exits.
Get-Process ClassroomStudent -ErrorAction SilentlyContinue | Stop-Process -Force
# Forget the saved policy so the next Student start does not re-apply it.
Get-ChildItem -Path "$env:APPDATA","$env:LOCALAPPDATA","$env:USERPROFILE" -Recurse -Filter guard-policy.json -ErrorAction SilentlyContinue | Remove-Item -Force
Write-Host 'Classroom Guard reset: DNS redirect removed, filters released, saved policy deleted.'
