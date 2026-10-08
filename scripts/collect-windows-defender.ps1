# Read-only evidence. Does not scan, restore files, change protection, or stop apps.
param(
 [string]$StudentPath,
 [string]$OutputDirectory = "$env:USERPROFILE\Desktop\Classroom-Defender-Evidence",
 [int]$Days = 14
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
function Save-Evidence($Name, [scriptblock]$Query) {
 try {
  @(& $Query) | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 (Join-Path $OutputDirectory "$Name.json")
 } catch {
  $_.Exception.Message | Set-Content -Encoding UTF8 (Join-Path $OutputDirectory "$Name.error.txt")
 }
}
Save-Evidence 'defender-status' {
 Get-MpComputerStatus | Select-Object AMServiceEnabled,AntivirusEnabled,RealTimeProtectionEnabled,AntivirusSignatureVersion,AntivirusSignatureLastUpdated
}
Save-Evidence 'threats' { Get-MpThreat }
Save-Evidence 'detections' { Get-MpThreatDetection }
Save-Evidence 'defender-events' {
 Get-WinEvent -FilterHashtable @{LogName='Microsoft-Windows-Windows Defender/Operational';Id=1116,1117;StartTime=(Get-Date).AddDays(-$Days)} -MaxEvents 100 |
  Select-Object TimeCreated,Id,Message
}
Save-Evidence 'application-errors' {
 Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000,1001;StartTime=(Get-Date).AddDays(-$Days)} -MaxEvents 100 |
  Where-Object { $_.Message -match 'Classroom' } | Select-Object TimeCreated,Id,Message
}
Save-Evidence 'classroom-processes' {
 Get-Process -Name Classroom,ClassroomStudent,ClassroomGuardService -ErrorAction SilentlyContinue |
  Select-Object Id,ProcessName,Path,StartTime
}
if ($StudentPath) {
 Save-Evidence 'student-file' {
  $file = Get-Item -LiteralPath $StudentPath
  $signature = Get-AuthenticodeSignature -LiteralPath $StudentPath
  [pscustomobject]@{
   Path=$file.FullName;Bytes=$file.Length
   SHA256=(Get-FileHash -LiteralPath $StudentPath -Algorithm SHA256).Hash
   SignatureStatus=[string]$signature.Status
   Publisher=$signature.SignerCertificate.Subject
  }
 }
}
Write-Output "Evidence: $OutputDirectory"
Write-Output 'Review event details before sharing; they can contain user and machine paths.'
