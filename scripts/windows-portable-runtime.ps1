# Shared packaging helpers. Ship the matching Microsoft redistributable DLLs
# app-local; classroom machines should not need developer tools or an installer.
function Get-ClassroomWindowsArchitecture([string]$Executable) {
    $reader = [IO.BinaryReader]::new([IO.File]::OpenRead($Executable))
    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) { throw 'Expected Windows executable' }
        $reader.BaseStream.Position = 0x3C
        $offset = $reader.ReadUInt32()
        $reader.BaseStream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550) { throw 'Invalid PE signature' }
        switch ($reader.ReadUInt16()) {
            0x8664 { return 'x64' }
            0xAA64 { return 'arm64' }
            default { throw 'Only x64 and ARM64 Windows executables are supported' }
        }
    } finally { $reader.Dispose() }
}
function Copy-ClassroomWindowsRuntime([string]$Architecture, [string]$Destination, [string]$RuntimeDirectory) {
    if (!$RuntimeDirectory) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        if (!(Test-Path -LiteralPath $vswhere)) { throw 'Set -RuntimeDirectory to the matching Visual C++ redist CRT directory' }
        $install = & $vswhere -latest -products '*' -property installationPath
        $redist = Join-Path $install 'VC/Redist/MSVC'
        $versions = Get-ChildItem -LiteralPath $redist -Directory | Where-Object Name -Match '^\d+\.' | Sort-Object { [version]$_.Name } -Descending
        foreach ($version in $versions) {
            $candidate = Get-ChildItem (Join-Path $version.FullName "$Architecture/Microsoft.VC*.CRT") -Directory -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($candidate) { $RuntimeDirectory = $candidate.FullName; break }
        }
    }
    if (!$RuntimeDirectory -or !(Test-Path -LiteralPath (Join-Path $RuntimeDirectory 'vcruntime140.dll'))) {
        throw "Visual C++ redistributable CRT for $Architecture is unavailable"
    }
    $runtimeArchitecture = Get-ClassroomWindowsArchitecture (Join-Path $RuntimeDirectory 'vcruntime140.dll')
    if ($runtimeArchitecture -ne $Architecture) { throw 'Visual C++ runtime architecture does not match executable' }
    Copy-Item (Join-Path $RuntimeDirectory '*.dll') -Destination $Destination -Force
}
