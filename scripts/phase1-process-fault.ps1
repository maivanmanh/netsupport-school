# Diagnostic only. Caller supplies the PID of its freshly deployed Classroom child.
param([int]$ProcessId, [ValidateSet('Suspend','Resume')][string]$Operation)
$ErrorActionPreference='Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ClassroomFault {
    [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
    [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle);
    [DllImport("ntdll.dll")] public static extern int NtSuspendProcess(IntPtr handle);
    [DllImport("ntdll.dll")] public static extern int NtResumeProcess(IntPtr handle);
}
'@
$process=Get-Process -Id $ProcessId
if ($process.ProcessName -ne 'Classroom') { throw 'Fault injection requires a Classroom process' }
$handle=[ClassroomFault]::OpenProcess(0x0800,$false,$ProcessId)
if ($handle -eq [IntPtr]::Zero) { throw 'Could not open owned Classroom process' }
try {
    $status=if ($Operation -eq 'Suspend') { [ClassroomFault]::NtSuspendProcess($handle) } else { [ClassroomFault]::NtResumeProcess($handle) }
    if ($status -ne 0) { throw "Fault injection failed: $status" }
} finally { $null=[ClassroomFault]::CloseHandle($handle) }
