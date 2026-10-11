# Cross-platform contract tests for the actual PowerShell enforcer functions.
# All OS-mutating commands are mocks. This is NOT Windows integration evidence.
$ErrorActionPreference='Stop'
$source=Join-Path $PSScriptRoot '../apps/classroom/src/hardware_control/windows.ps1'
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($source,[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
$functions=$ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst]},$true)
foreach ($fn in $functions | Where-Object Name -in @('Storage','Journal','Restore','Block-Network','Block-Pnp','Require-Service')) {
    . ([scriptblock]::Create($fn.Extent.Text))
}
function Assert($condition,$message) { if (-not $condition) {throw $message} }
function Reset {
    $script:state=[pscustomobject]@{devices=@();registry=@();usb_boot='';usb_restore_boot='';teacher='10.0.55.200';service_seen=1000}
    $script:boot='boot1'; $script:regPath='MockRegistry'
    $script:adapters=@(); $script:pnp=@(); $script:disabled=@(); $script:enabled=@(); $script:saved=0
    $script:routeIndex=1; $script:registry=@{}; $script:parents=@{}
    $script:disks=@(); $script:compat=@{}; $script:serviceStatus='Running'
}
function Get-Service {param($Name) [pscustomobject]@{Status=$script:serviceStatus}}
function Get-CimInstance { param($ClassName) $script:disks }
function Save-State { $script:saved++ }
function Get-NetAdapter { param([switch]$IncludeHidden,[Parameter(ValueFromPipeline)]$InputObject) process { if ($InputObject) {$InputObject} else {$script:adapters} } }
function Find-NetRoute { param($RemoteIPAddress) [pscustomobject]@{InterfaceIndex=$script:routeIndex} }
function Disable-NetAdapter { param([Parameter(ValueFromPipeline)]$InputObject,$Confirm) process {
    Assert ($script:saved -gt 0) 'Mutation before journal persistence'
    $script:disabled += $InputObject.Name; $InputObject.AdminStatus='Down'
} }
function Enable-NetAdapter { param([Parameter(ValueFromPipeline)]$InputObject,$Confirm) process { $script:enabled += $InputObject.Name; $InputObject.AdminStatus='Up' } }
function Get-PnpDevice { param($InstanceId,[switch]$PresentOnly) if ($InstanceId) { $script:pnp | Where-Object InstanceId -eq $InstanceId } else {$script:pnp} }
function Get-PnpDeviceProperty { param($InstanceId,$KeyName) switch ($KeyName) {
    'DEVPKEY_Device_CompatibleIds' { [pscustomobject]@{Data=$(if ($script:compat.ContainsKey($InstanceId)) {$script:compat[$InstanceId]} else {@('USB\Class_E0&SubClass_01&Prot_03')})} }
    'DEVPKEY_Device_ProblemCode' { [pscustomobject]@{Data=($script:pnp | Where-Object InstanceId -eq $InstanceId).Problem} }
    'DEVPKEY_Device_Parent' { [pscustomobject]@{Data=$script:parents[$InstanceId]} }
} }
function Disable-PnpDevice { param([Parameter(ValueFromPipeline)]$InputObject,$Confirm) process {
    Assert ($script:saved -gt 0) 'PnP mutation before journal'
    $script:disabled += $InputObject.InstanceId; $InputObject.Problem=22
} }
function Enable-PnpDevice { param([Parameter(ValueFromPipeline)]$InputObject,$Confirm) process {$script:enabled += $InputObject.InstanceId; $InputObject.Problem=0} }
function Get-ItemProperty { param($LiteralPath,$Name) if ($script:registry.ContainsKey($Name)) { $v=@{}; $v[$Name]=$script:registry[$Name]; [pscustomobject]$v } }
function New-Item { param($Path,[switch]$Force) }
function New-ItemProperty { param($LiteralPath,$Name,$Value,$PropertyType,[switch]$Force) $script:registry[$Name]=$Value }
function Remove-ItemProperty {param($LiteralPath,$Name) $script:registry.Remove($Name)}
function Adapter($name,$index,$medium,$id) { [pscustomobject]@{Name=$name;ifIndex=$index;NdisPhysicalMedium=$medium;PnPDeviceID=$id;InterfaceGuid=$name;AdminStatus='Up';HardwareInterface=$true} }
function Device($class,$id,$problem=0) { [pscustomobject]@{Class=$class;InstanceId=$id;FriendlyName=$id;Problem=$problem;Present=$true} }

Reset
Require-Service 1000
$state.service_seen=0
try { Require-Service 1000; throw 'Old service accepted' } catch { Assert ($_.Exception.Message -like '*hardware heartbeat*') 'Old/missing hardware service must be rejected' }
$state.service_seen=1000; $script:serviceStatus='Stopped'
try { Require-Service 1000; throw 'Stopped service accepted' } catch { Assert ($_.Exception.Message -like '*hardware heartbeat*') 'Stopped service must be rejected' }
$script:serviceStatus='Running'
$script:adapters=@((Adapter LAN 1 14 'PCI\LAN'),(Adapter WiFi 2 9 'PCI\WIFI'),(Adapter Phone 3 14 'USB\PHONE'))
[void](Block-Network wifi)
Assert ($script:disabled.Count -eq 1 -and $script:disabled[0] -eq 'WiFi') 'Wi-Fi must preserve wired LAN and tether'
[void](Restore wifi)
Assert ($script:enabled[0] -eq 'WiFi' -and $state.devices.Count -eq 0) 'Owned adapter must restore'
$script:routeIndex=2
try { [void](Block-Network wifi); throw 'Did not refuse active management route' } catch { Assert ($_.Exception.Message -like '*management connection*') 'Route refusal missing' }
Assert ($script:disabled.Count -eq 1) 'Refused operation must not mutate'
$script:routeIndex=1
$script:adapters[0].HardwareInterface=$false
try { [void](Block-Network usb_tethering); throw 'Did not refuse virtual route' } catch { Assert ($_.Exception.Message -like '*virtual adapter*') 'Virtual route refusal missing' }
$script:adapters[0].HardwareInterface=$true
[void](Block-Network usb_tethering)
Assert ($script:disabled[-1] -eq 'Phone') 'Tether must disable only matching USB network adapter'
$script:adapters += Adapter UsbEthernet 4 14 'USB\ETHERNET'
$script:compat['USB\ETHERNET']=@('USB\Class_FF&SubClass_00')
Assert ((Block-Network usb_tethering).status -eq 'unsupported') 'Unknown USB adapter must not claim tether coverage'
Assert ($script:adapters[-1].AdminStatus -eq 'Up') 'Preserve unclassified USB Ethernet'

Reset
$script:pnp=@((Device Camera 'USB\CAM'),(Device AudioEndpoint 'SWD\MMDEVAPI\{0.0.1.MIC}'),(Device AudioEndpoint 'SWD\MMDEVAPI\{0.0.0.SPEAKER}'),(Device Keyboard 'USB\KEY'),(Device Bluetooth 'USB\BT' 22))
[void](Block-Pnp camera_microphone)
Assert ($script:disabled.Count -eq 2 -and 'USB\CAM' -in $script:disabled -and 'SWD\MMDEVAPI\{0.0.1.MIC}' -in $script:disabled) 'Capture devices only; preserve speakers and keyboard'
[void](Block-Pnp bluetooth)
Assert ($state.devices.Count -eq 2) 'Already disabled owner device must not enter restore journal'
$script:pnp[0].Present=$false
$r=Restore camera_microphone
Assert ($r.status -eq 'pending' -and $state.devices.Count -eq 1) 'Unplugged owned device must retain restoration entry'
$script:pnp[0].Present=$true
[void](Restore camera_microphone)
Assert ($state.devices.Count -eq 0) 'Replug must finish pending restoration'
$script:pnp=@((Device Keyboard 'HID\K'),(Device Bluetooth 'USB\BT'))
$script:parents['HID\K']='BTHENUM\K'
try { [void](Block-Pnp bluetooth); throw 'Did not preserve Bluetooth keyboard' } catch { Assert ($_.Exception.Message -like '*preserve keyboard*') 'Bluetooth HID ancestry refusal missing' }
Assert ($script:disabled.Count -eq 2) 'Bluetooth refusal must not disable radio'

Reset
$script:registry['Deny_Write']=0
$r=Storage $true
Assert ($r.status -eq 'pending' -and $script:registry['Deny_Read'] -eq 1 -and $script:registry['Deny_Write'] -eq 1) 'Storage saves both read/write policies without forcing dismount'
$script:boot='boot2'
Assert ((Storage $true).status -eq 'applied') 'Reboot reconciles storage'
[void](Storage $false)
Assert (-not $script:registry.ContainsKey('Deny_Read') -and $script:registry['Deny_Write'] -eq 0) 'Restore exact prior registry baseline'
Assert ((Storage $false).status -eq 'pending') 'Do not claim immediate OS cache restoration'
$script:boot='boot3'
Assert ((Storage $false).status -eq 'applied') 'Restore settles after restart'
$script:disks=@([pscustomobject]@{InterfaceType='USB';MediaType='Fixed hard disk media'})
Assert ((Storage $true).status -eq 'unsupported') 'Fixed USB media must not claim guaranteed coverage'
Write-Host 'Hardware enforcer contracts passed: LAN protection, virtual route, tether classification, journal-before-mutation, camera/mic, speaker/input preservation, pre-disabled baseline, hotplug restore, Bluetooth ancestry, storage read/write and reboot.'
