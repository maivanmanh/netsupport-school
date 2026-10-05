param([Parameter(Mandatory=$true)][string]$Root, [string]$StudentIp='192.168.50.206')
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$descriptor=Get-Content -Raw (Join-Path $Root 'teacher-endpoint.json') | ConvertFrom-Json
$client=[Net.Sockets.TcpClient]::new('127.0.0.1',[int]$descriptor.port)
$stream=$client.GetStream(); $stream.ReadTimeout=10000
function Read-Exact([int]$size) {
  $bytes=[byte[]]::new($size); $offset=0
  while($offset -lt $size){$n=$stream.Read($bytes,$offset,$size-$offset);if(!$n){throw 'Teacher disconnected'};$offset+=$n}
  return ,$bytes
}
function Send-Json($value){
  $bytes=[Text.Encoding]::UTF8.GetBytes(($value | ConvertTo-Json -Depth 12 -Compress))
  $header=[BitConverter]::GetBytes([Net.IPAddress]::HostToNetworkOrder([int]$bytes.Length))
  $stream.Write($header,0,4);$stream.Write($bytes,0,$bytes.Length)
}
function Read-Json {
  $header=Read-Exact 4;[Array]::Reverse($header);$size=[BitConverter]::ToInt32($header,0)
  if($size -lt 1 -or $size -gt 8388608){throw 'Invalid IPC frame'}
  $value=[Text.Encoding]::UTF8.GetString((Read-Exact $size)) | ConvertFrom-Json
  if($value.type -eq 'state'){$script:state=$value}
  return $value
}
$script:counter=0
function Command([string]$action,$selected,$inputValue){
  $script:counter++;$id=$script:counter
  Send-Json @{id=$id;action=$action;selected=@($selected);input=$inputValue}
  do{$value=Read-Json}while($value.type -ne 'ack' -or $value.id -ne $id)
  if(!$value.ok){throw "Command $action failed: $($value.error)"}
}
try {
  $previousError=Join-Path $Root 'forwarding-error.txt'
  if(Test-Path $previousError){Move-Item $previousError (Join-Path $Root ('forwarding-error-'+(Get-Date -Format 'yyyyMMdd-HHmmssfff')+'.txt'))}
  Send-Json @{type='hello';protocol=1;token=$descriptor.token;ui_role='automation'}
  $null=Read-Json
  $end=(Get-Date).AddSeconds(30)
  do {
    $student=$script:state.students | Where-Object {$_.ip -eq $StudentIp -and $_.online} | Select-Object -First 1
    if($student){break};$null=Read-Json
  } while((Get-Date) -lt $end)
  if(!$student){throw 'Student not online'}
  "Student online: $($student.name)" | Set-Content (Join-Path $Root 'forwarding-lifecycle.txt')
  $arguments=@('--observe-id',$student.id,'--observe-name',$student.name,'--ipc-file',(Join-Path $Root 'teacher-endpoint.json'))
  $null=Start-Process (Join-Path $Root 'Teacher/classroom_console.exe') -ArgumentList $arguments -WorkingDirectory (Join-Path $Root 'Teacher') -PassThru
  $end=(Get-Date).AddSeconds(30)
  do {$null=Read-Json;if($script:state.observe.phase -eq 'live'){break}}while((Get-Date) -lt $end)
  if($script:state.observe.phase -ne 'live'){throw 'Observe did not become live'}
  Start-Sleep -Seconds 3
  Command 'observe_input' @($student.id) @{action='control'}
  $end=(Get-Date).AddSeconds(10)
  do {$null=Read-Json;if($script:state.observe.mode -eq 'control'){break}}while((Get-Date) -lt $end)
  if($script:state.observe.mode -ne 'control'){throw 'Control did not become active'}
  'Control active' | Add-Content (Join-Path $Root 'forwarding-lifecycle.txt')
  Start-Sleep -Seconds 2
  $test=Start-Process (Join-Path $Root 'shortcut-forwarding-test.exe') -ArgumentList (Join-Path $Root 'forwarding-result.txt') -Wait -PassThru
  "Injection test exit: $($test.ExitCode)" | Add-Content (Join-Path $Root 'forwarding-lifecycle.txt')
  if($test.ExitCode -ne 0){throw 'Shortcut injection failed; inspect forwarding-result.txt'}
  # Leave GUI and Control running for owner inspection. No raw IPC token logged.
} catch {
  $_.ToString() | Set-Content (Join-Path $Root 'forwarding-error.txt')
  exit 1
} finally { $client.Dispose() }
