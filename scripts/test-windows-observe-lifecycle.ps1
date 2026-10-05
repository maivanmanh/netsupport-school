# IPC lifecycle test; run in an interactive scheduled task. Foreground activation from an actual UI click requires manual validation.
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
  if(!$value.ok){throw "Command $action failed: $($value.error)"};return $value
}
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class ObserveTestWindow {
 public delegate bool EnumProc(IntPtr h, IntPtr data);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr data);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 public static IntPtr Find(uint pid) { IntPtr found=IntPtr.Zero; EnumWindows((h,d)=>{uint p;GetWindowThreadProcessId(h,out p);if(p==pid&&IsWindowVisible(h))found=h;return true;},IntPtr.Zero);return found; }
}
'@
function OwnedObserve {
 @(Get-CimInstance Win32_Process | Where-Object {$_.ExecutablePath -and $_.ExecutablePath.Replace('\','/').StartsWith($Root+'/Teacher/',[StringComparison]::OrdinalIgnoreCase) -and $_.CommandLine -match '--observe-id' -and $_.CommandLine.Contains($student.id)})
}
$report=Join-Path $Root 'observe-tools-test.txt'
try {
 Send-Json @{type='hello';protocol=1;token=$descriptor.token;ui_role='automation'}
 $null=Read-Json
 $end=(Get-Date).AddSeconds(30)
 do {
  $student=$script:state.students | Where-Object {$_.ip -eq $StudentIp -and $_.online} | Select-Object -First 1
  if($student){break};$null=Read-Json
 } while((Get-Date) -lt $end)
 if(!$student){throw 'Student not connected'}
 if($script:state.settings.tools.max_file_mb -ne 300){throw 'Default limit is not 300 MB'}
 'Default file limit: 300 MB' | Set-Content $report
 $clock=[Diagnostics.Stopwatch]::StartNew()
 $null=Command 'observe_window' @($student.id) @{}
 $null=Command 'observe_window' @($student.id) @{}
 $end=(Get-Date).AddSeconds(15);$handle=[IntPtr]::Zero
 do {
  $owned=@(OwnedObserve)
  if($owned.Count -gt 1){throw 'Duplicate Observe processes'}
  if($owned.Count -eq 1){$handle=[ObserveTestWindow]::Find([uint32]$owned[0].ProcessId)}
  if($handle -ne [IntPtr]::Zero){break}
  Start-Sleep -Milliseconds 50
 } while((Get-Date) -lt $end)
 if($handle -eq [IntPtr]::Zero){throw 'Observe window not visible'}
 "One Observe window visible after $($clock.ElapsedMilliseconds) ms; repeated startup click reused it" | Add-Content $report
 $end=(Get-Date).AddSeconds(30)
 do {$null=Read-Json;if($script:state.observe.phase -eq 'live'){break}} while((Get-Date) -lt $end)
 if($script:state.observe.phase -ne 'live'){throw 'Observe did not become live'}
 'Observe video live' | Add-Content $report
 $null=Command 'observe_window' @($student.id) @{}
 Start-Sleep -Milliseconds 800
 if(@(OwnedObserve).Count -ne 1){throw 'Repeated live click duplicated window'}
 "Repeated live click: one process; foreground matches: $([ObserveTestWindow]::GetForegroundWindow() -eq $handle)" | Add-Content $report
 $folder=Join-Path $Root 'Observe-tools-smoke'
 New-Item -ItemType Directory -Path $folder -Force | Out-Null
 'Classroom Observe transfer validation' | Set-Content (Join-Path $folder 'notes.txt')
 $name='Classroom-Observe-tools-'+(Get-Date -Format 'yyyyMMdd-HHmmss')+'.zip'
 $zip=Join-Path $Root ('outbox/'+$name)
 Compress-Archive -LiteralPath $folder -DestinationPath $zip
 $ack=Command 'tool_file' @($student.id) @{path=$zip;name=$name}
 $end=(Get-Date).AddSeconds(30)
 do {
  $null=Read-Json
  $job=$script:state.tools.jobs | Where-Object {$_.id -eq $ack.job_id} | Select-Object -First 1
  if($job -and $job.targets[0].status -eq 'complete'){break}
  if($job -and $job.targets[0].status -eq 'error'){throw $job.targets[0].error}
 } while((Get-Date) -lt $end)
 if(!$job -or $job.targets[0].status -ne 'complete'){throw 'ZIP transfer did not complete'}
 "ZIP received: $name; bytes $($job.size); Student $($student.name)" | Add-Content $report
 Start-Sleep -Seconds 8
 $clock.Restart()
 $null=[ObserveTestWindow]::PostMessage($handle,0x0010,[IntPtr]::Zero,[IntPtr]::Zero)
 while([ObserveTestWindow]::IsWindowVisible($handle) -and $clock.ElapsedMilliseconds -lt 3000){Start-Sleep -Milliseconds 10}
 "X hides window after $($clock.ElapsedMilliseconds) ms" | Add-Content $report
 if([ObserveTestWindow]::IsWindowVisible($handle) -or $clock.ElapsedMilliseconds -gt 500){throw 'Close did not hide immediately'}
 Start-Sleep -Seconds 2
 if(@(OwnedObserve).Count -ne 0){throw 'Observe process did not exit'}
 'Observe process exited; Teacher still connected' | Add-Content $report
 $null=Command 'observe_window' @($student.id) @{}
 'Fresh Observe opened for owner inspection' | Add-Content $report
 'PASS' | Add-Content $report
} catch { $_.ToString() | Add-Content $report;exit 1 }
finally {$client.Dispose()}
