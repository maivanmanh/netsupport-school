param([Parameter(Mandatory=$true)][string]$RustAgent)
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
Push-Location "$root/apps/classroom_console"
try {
 flutter pub get
 if($LASTEXITCODE -ne 0){throw 'flutter pub get failed'}
 flutter build windows --release
 if($LASTEXITCODE -ne 0){throw 'flutter Windows build failed'}
 $destination="$root/target/Classroom-Console-Windows-x64"
 New-Item -ItemType Directory -Force $destination | Out-Null
 Copy-Item 'build/windows/x64/runner/Release/*' $destination -Recurse -Force
 Copy-Item $RustAgent "$destination/ClassroomAgent.exe" -Force
 Write-Output "Bundle: $destination"
} finally { Pop-Location }
