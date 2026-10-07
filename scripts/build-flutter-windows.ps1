param([string]$RustAgent, [string]$RuntimeDirectory, [string]$OutputDirectory)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/windows-portable-runtime.ps1"
$root=Split-Path $PSScriptRoot -Parent
# Legacy RustAgent argument remains accepted; release builds use fresh paired roles.
Push-Location $root
try {
 $env:CARGO_TARGET_DIR="$root/target"
 if(-not $env:CLASSROOM_BUILD_LABEL){$env:CLASSROOM_BUILD_LABEL='paired-'+(Get-Date -Format 'yyyyMMdd-HHmmss')}
 cargo build --release --bin Classroom --bin ClassroomStudent --target x86_64-pc-windows-msvc
 if($LASTEXITCODE -ne 0){throw 'Paired Rust build failed; refusing stale Student package'}
 $RustAgent="$root/target/x86_64-pc-windows-msvc/release/Classroom.exe"
 python scripts/classroom-dev-update.py package "$root/target/x86_64-pc-windows-msvc/release/ClassroomStudent.exe" --version $env:CLASSROOM_BUILD_LABEL --output "$root/target/updates/student-windows-x64.zip"
 if($LASTEXITCODE -ne 0){throw 'Student package failed'}
} finally {Pop-Location}
$RustAgent=(Resolve-Path -LiteralPath $RustAgent).Path
if ((Get-ClassroomWindowsArchitecture $RustAgent) -ne 'x64') { throw 'The x64 Flutter bundle requires an x64 Rust agent' }
Push-Location "$root/apps/classroom_console"
try {
 flutter pub get
 if($LASTEXITCODE -ne 0){throw 'flutter pub get failed'}
 flutter build windows --release
 if($LASTEXITCODE -ne 0){throw 'flutter Windows build failed'}
 $destination=if($OutputDirectory){$OutputDirectory}else{"$root/target/Classroom-Console-Windows-x64"}
 New-Item -ItemType Directory -Force $destination | Out-Null
 Copy-Item 'build/windows/x64/runner/Release/*' $destination -Recurse -Force
 Copy-ClassroomWindowsRuntime 'x64' $destination $RuntimeDirectory
 Copy-Item $RustAgent "$destination/ClassroomAgent.exe" -Force
 $student=Join-Path (Split-Path $RustAgent -Parent) 'ClassroomStudent.exe'
 if(-not(Test-Path -LiteralPath $student)){throw 'Fresh Student executable is missing'}
 Copy-Item -LiteralPath $student "$destination/ClassroomStudent.exe" -Force
 $package="$root/target/updates/student-windows-x64.zip"
 if(-not(Test-Path -LiteralPath $package)){throw 'Fresh Student package is missing'}
 New-Item -ItemType Directory -Force "$destination/student-packages" | Out-Null
 Copy-Item -LiteralPath $package "$destination/student-packages/student-windows-x64.zip" -Force
 Write-Output "Bundle: $destination"
} finally { Pop-Location }
