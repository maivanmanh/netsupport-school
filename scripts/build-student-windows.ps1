param(
    [Parameter(Mandatory=$true)][string]$RustStudent,
    [string]$RuntimeDirectory
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/windows-portable-runtime.ps1"
$root = Split-Path $PSScriptRoot -Parent
$binary = (Resolve-Path -LiteralPath $RustStudent).Path
$architecture = Get-ClassroomWindowsArchitecture $binary
$destination = Join-Path $root "target/Classroom-Student-Windows-$architecture"
New-Item -ItemType Directory -Force $destination | Out-Null
Copy-ClassroomWindowsRuntime $architecture $destination $RuntimeDirectory
Copy-Item -LiteralPath $binary -Destination (Join-Path $destination 'ClassroomStudent.exe') -Force
Write-Output "Bundle: $destination"
