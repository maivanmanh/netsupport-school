# Fixed privileged reconciler. Input is data on stdin, never executable text.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$root = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'Classroom\HardwareControl'
$file = Join-Path $root 'state.json'
if (-not $request.policy -and -not $request.service -and -not (Test-Path -LiteralPath $file)) {
    [Console]::Write('{}'); exit 0
}
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Administrator rights required' }

# Do not follow a user-planted junction into unrelated data. Restrict durable
# desired state, rollback journal and reports to SYSTEM and Administrators.
$parent = Split-Path $root
foreach ($path in @($parent, $root)) {
    if (Test-Path -LiteralPath $path) {
        if ((Get-Item -LiteralPath $path).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Hardware policy path is a reparse point' }
    } else { [void][IO.Directory]::CreateDirectory($path) }
}
$acl = New-Object Security.AccessControl.DirectorySecurity
$acl.SetAccessRuleProtection($true,$false)
$acl.SetOwner((New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')))
foreach ($sid in @('S-1-5-18','S-1-5-32-544')) {
    $rule = New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier($sid)), 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
    $acl.AddAccessRule($rule)
}
Set-Acl -LiteralPath $root -AclObject $acl
if ((Test-Path -LiteralPath $file) -and ((Get-Item -LiteralPath $file).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Hardware journal is a reparse point' }
$fileAcl = New-Object Security.AccessControl.FileSecurity
$fileAcl.SetAccessRuleProtection($true,$false)
$fileAcl.SetOwner((New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')))
foreach ($sid in @('S-1-5-18','S-1-5-32-544')) {
    $fileAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier($sid)), 'FullControl', 'Allow')))
}
if (Test-Path -LiteralPath $file) {
    Set-Acl -LiteralPath $file -AclObject $fileAcl
}
$security = New-Object Security.AccessControl.MutexSecurity
foreach ($sid in @('S-1-5-18','S-1-5-32-544')) {
    $security.AddAccessRule((New-Object Security.AccessControl.MutexAccessRule((New-Object Security.Principal.SecurityIdentifier($sid)), 'FullControl', 'Allow')))
}
$created = $false
$mutex = New-Object Threading.Mutex($false, 'Global\ClassroomHardwarePolicy', [ref]$created, $security)
$owned = $false
try {
    try { $owned = $mutex.WaitOne(5000) } catch [Threading.AbandonedMutexException] { $owned = $true }
    if (-not $owned) { throw 'Hardware policy busy; retry' }
    $boot = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o')
    if (Test-Path -LiteralPath $file) { $state = Get-Content -Raw -LiteralPath $file | ConvertFrom-Json }
    else { $state = [pscustomobject]@{ policy=$null; teacher=$null; devices=@(); registry=@(); usb_boot=''; usb_restore_boot=''; service_seen=0; report=$null } }
    if (-not $state.PSObject.Properties['usb_restore_boot']) { $state | Add-Member -NotePropertyName usb_restore_boot -NotePropertyValue '' }
    if (-not $state.PSObject.Properties['service_seen']) { $state | Add-Member -NotePropertyName service_seen -NotePropertyValue 0 }
    function Save-State {
        $temp = Join-Path $root ([Guid]::NewGuid().ToString('N') + '.tmp')
        $bytes = [Text.Encoding]::UTF8.GetBytes(($state | ConvertTo-Json -Depth 12 -Compress))
        $stream = New-Object IO.FileStream($temp,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try { $stream.Write($bytes,0,$bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
        Set-Acl -LiteralPath $temp -AclObject $fileAcl
        if (Test-Path -LiteralPath $file) { [IO.File]::Replace($temp,$file,$null) } else { [IO.File]::Move($temp,$file) }
    }
    function Require-Service($now) {
        if ((Get-Service -Name ClassroomGuardService).Status -ne 'Running' -or $now-[long]$state.service_seen -gt 70 -or [long]$state.service_seen -gt $now+5) { throw 'Updated Classroom Guard service must be running with a fresh hardware heartbeat; update/restart service and retry' }
    }
    $now = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    if ($request.service) { $state.service_seen=$now; Save-State }
    elseif (-not $request.recovery -and ($request.policy -or $state.policy)) {
        Require-Service $now
    }
    if ($request.policy) {
        if ($request.policy.revision -notmatch '^[a-fA-F0-9]{32}$') { throw 'Invalid revision' }
        $state.policy = $request.policy
        $state.teacher = [string]$request.teacher
        # Persist intent before touching any device. Service will retry after crash.
        Save-State
    }
    if (-not $state.policy) { [Console]::Write('{}'); exit 0 }
    $results = @{}
    $regPath = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\RemovableStorageDevices\{53f5630d-b6bf-11d0-94f2-00a0c91efb8b}'
    function Storage($blocked) {
        if ($blocked) {
            if (@($state.registry).Count -eq 0) {
                foreach ($name in @('Deny_Read','Deny_Write')) {
                    $old = Get-ItemProperty -LiteralPath $regPath -Name $name -ErrorAction SilentlyContinue
                    $state.registry += [pscustomobject]@{name=$name; exists=($null -ne $old); value=$(if ($old) { $old.$name } else { 0 })}
                }
                $state.usb_boot = $boot
                Save-State
            }
            [void](New-Item -Path $regPath -Force)
            foreach ($name in @('Deny_Read','Deny_Write')) { [void](New-ItemProperty -LiteralPath $regPath -Name $name -Value 1 -PropertyType DWord -Force) }
            # No disk removal, dismount, driver stop or forced reboot: open file
            # handles may survive policy change. Never claim immediate revocation.
            $fixedUsb = @(Get-CimInstance Win32_DiskDrive | Where-Object { $_.InterfaceType -eq 'USB' -and $_.MediaType -match 'Fixed' })
            if ($fixedUsb.Count) { return @{status='unsupported'; error='Fixed-media USB disk detected. Removable-disk policy cannot guarantee coverage for this driver; no forced offline/dismount to protect data.'} }
            if ($state.usb_boot -eq $boot) { return @{status='pending'; error='Read/write policy saved. Existing open handles may remain usable until restart. No forced dismount/reboot.'} }
        } elseif (@($state.registry).Count -gt 0) {
            foreach ($old in @($state.registry)) {
                $current = Get-ItemProperty -LiteralPath $regPath -Name $old.name -ErrorAction SilentlyContinue
                # Another administrator/GPO owns divergent values: do not overwrite.
                $alreadyRestored = ($old.exists -and $current -and $current.($old.name) -eq $old.value) -or (-not $old.exists -and -not $current)
                if (-not $alreadyRestored -and $current -and $current.($old.name) -ne 1) { throw ('Storage policy changed externally: ' + $old.name) }
                if ($old.exists) { [void](New-ItemProperty -LiteralPath $regPath -Name $old.name -Value $old.value -PropertyType DWord -Force) }
                else { Remove-ItemProperty -LiteralPath $regPath -Name $old.name -ErrorAction SilentlyContinue }
                $state.registry = @($state.registry | Where-Object name -ne $old.name)
                Save-State
            }
            $state.registry=@(); $state.usb_boot=''; $state.usb_restore_boot=$boot; Save-State
        }
        if (-not $blocked -and $state.usb_restore_boot -eq $boot) { return @{status='pending'; error='Original storage policy restored. Restart may be required for cached OS access decisions; no forced reboot.'} }
        return @{status='applied'; error=''}
    }
    function Journal($group,$kind,$id) {
        if (-not @($state.devices | Where-Object { $_.kind -eq $kind -and $_.id -eq $id }).Count) {
            $state.devices += [pscustomobject]@{group=$group; kind=$kind; id=$id}
            Save-State
        }
    }
    function Restore($group) {
        $pending = $false
        foreach ($entry in @($state.devices | Where-Object group -eq $group)) {
            if ($entry.kind -eq 'net') {
                $adapter = @(Get-NetAdapter -IncludeHidden | Where-Object { [string]$_.InterfaceGuid -eq $entry.id })
                if (-not $adapter.Count) { $pending=$true; continue }
                $adapter | Enable-NetAdapter -Confirm:$false
                if (@($adapter | Get-NetAdapter | Where-Object AdminStatus -ne 'Up').Count) { throw 'Adapter did not enable' }
            } else {
                $device = Get-PnpDevice -InstanceId $entry.id -ErrorAction SilentlyContinue
                if (-not $device -or $device.Present -eq $false) { $pending=$true; continue }
                $device | Enable-PnpDevice -Confirm:$false
                if ((Get-PnpDeviceProperty -InstanceId $entry.id -KeyName 'DEVPKEY_Device_ProblemCode').Data -eq 22) { throw ('Device still disabled: ' + $entry.id) }
            }
            $state.devices = @($state.devices | Where-Object { -not ($_.kind -eq $entry.kind -and $_.id -eq $entry.id) })
            Save-State
        }
        if ($pending) { return @{status='pending'; error='Restore retained for disconnected device; it will be enabled when reconnected.'} }
        return @{status='applied'; error=''}
    }
    function Block-Network($group) {
        $adapters = @(Get-NetAdapter -IncludeHidden)
        $targets = @()
        $unknownUsb = $false
        foreach ($a in $adapters) {
            if ($group -eq 'wifi') {
                if ([int]$a.NdisPhysicalMedium -in @(1,9)) { $targets += $a }
            } elseif ($a.PnPDeviceID -like 'USB\*') {
                $ids = @((Get-PnpDeviceProperty -InstanceId $a.PnPDeviceID -KeyName DEVPKEY_Device_CompatibleIds -ErrorAction SilentlyContinue).Data)
                # Only positively identified RNDIS / CDC ECM / CDC NCM or Apple
                # mobile networking; never all USB Ethernet or USB controllers.
                if (($ids -match 'USB\\(MS_COMP_RNDIS|Class_(E0&SubClass_01&Prot_03|EF&SubClass_04&Prot_01|02&SubClass_(06|0D)))') -or $a.PnPDeviceID -match '^USB\\VID_05AC&') { $targets += $a }
                else { $unknownUsb = $true }
            }
        }
        $up = @($targets | Where-Object AdminStatus -eq 'Up')
        if ($up.Count) {
            if (-not $state.teacher) { throw 'Teacher route unknown; refusing network disable' }
            $route = @(Find-NetRoute -RemoteIPAddress $state.teacher -ErrorAction Stop)
            $protected = @($route | ForEach-Object InterfaceIndex)
            if (-not $protected.Count) { throw 'Cannot verify management route' }
            if (@($up | Where-Object { $_.ifIndex -in $protected }).Count) { throw 'Refused: adapter carries the management connection to Teacher; connect wired LAN first' }
            $management = @($adapters | Where-Object { $_.ifIndex -in $protected })
            if (-not $management.Count -or @($management | Where-Object { -not $_.HardwareInterface }).Count) { throw 'Refused: management uses an unknown/virtual adapter; physical LAN independence cannot be verified' }
            foreach ($a in $up) {
                Journal $group 'net' ([string]$a.InterfaceGuid)
                $a | Disable-NetAdapter -Confirm:$false
                if ((Get-NetAdapter -IncludeHidden | Where-Object InterfaceGuid -eq $a.InterfaceGuid).AdminStatus -ne 'Down') { throw ('Adapter did not disable: ' + $a.Name) }
            }
        }
        if ($unknownUsb) { return @{status='unsupported'; error='Unclassified USB network adapter preserved. Cannot guarantee tether blocking without identifying its driver; USB Ethernet is never disabled by guesswork.'} }
        return @{status='applied'; error=''}
    }
    function Block-Pnp($group) {
        $present = @(Get-PnpDevice -PresentOnly)
        if ($group -eq 'bluetooth') {
            # Preserve essential Bluetooth keyboards/mice, including composite
            # HID children. Conservative refusal is safer than losing input.
            foreach ($inputDevice in @($present | Where-Object { $_.Class -in @('Keyboard','Mouse','HIDClass') })) {
                $id = $inputDevice.InstanceId
                for ($depth=0; $depth -lt 8 -and $id; $depth++) {
                    if ($id -match 'BTH|BLUETOOTH') { throw 'Refused: Bluetooth input device present; preserve keyboard/mouse' }
                    $id = (Get-PnpDeviceProperty -InstanceId $id -KeyName DEVPKEY_Device_Parent -ErrorAction Stop).Data
                }
                if ($id) { throw 'Input ancestry could not be verified; refusing Bluetooth disable' }
            }
            $targets = @($present | Where-Object { $_.Class -eq 'Bluetooth' -and $_.InstanceId -match '^(USB|PCI|ACPI)\\' })
            if (@(Get-NetAdapter -IncludeHidden | Where-Object { [int]$_.NdisPhysicalMedium -eq 10 -and $_.AdminStatus -eq 'Up' }).Count) { throw 'Refused: active Bluetooth network adapter may carry management traffic' }
        } else {
            $targets = @($present | Where-Object { $_.Class -eq 'Camera' -or ($_.Class -eq 'Image' -and @((Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName DEVPKEY_Device_CompatibleIds -ErrorAction SilentlyContinue).Data) -match 'USB\\Class_0E') -or ($_.Class -eq 'AudioEndpoint' -and $_.InstanceId -match '^SWD\\MMDEVAPI\\\{0\.0\.1\.') })
        }
        foreach ($d in $targets) {
            $problem = (Get-PnpDeviceProperty -InstanceId $d.InstanceId -KeyName DEVPKEY_Device_ProblemCode).Data
            if ($problem -eq 22) { continue } # Preserve devices disabled by owner.
            Journal $group 'pnp' $d.InstanceId
            $d | Disable-PnpDevice -Confirm:$false
            if ((Get-PnpDeviceProperty -InstanceId $d.InstanceId -KeyName DEVPKEY_Device_ProblemCode).Data -ne 22) { throw ('Device did not disable: ' + $d.FriendlyName) }
        }
        return @{status='applied'; error=''}
    }
    foreach ($group in @('usb_storage','wifi','bluetooth','usb_tethering','camera_microphone')) {
        $blocked = [bool]$state.policy.$group
        try {
            if ($group -eq 'usb_storage') { $result=Storage $blocked }
            elseif (-not $blocked) { $result=Restore $group }
            elseif ($group -in @('wifi','usb_tethering')) { $result=Block-Network $group }
            else { $result=Block-Pnp $group }
        } catch {
            $message=[string]$_.Exception.Message
            $result=@{status='failed'; error=$message.Substring(0,[Math]::Min(200,$message.Length))}
        }
        $result.blocked=$blocked
        $results[$group]=$result
    }
    $state.report=@{revision=$state.policy.revision; groups=$results}
    Save-State
    [Console]::Write(($state.report | ConvertTo-Json -Depth 8 -Compress))
} finally {
    if ($owned) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
