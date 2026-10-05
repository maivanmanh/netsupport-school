param([Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference='Stop'
# Invoke in the logged-in interactive desktop, not in the SSH service session.
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class ClassroomRunCheck {
 delegate bool Callback(IntPtr h,IntPtr p);
 [DllImport("user32.dll")] static extern bool EnumWindows(Callback c,IntPtr p);
 [DllImport("user32.dll")] static extern int GetWindowText(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")] static extern int GetClassName(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
 public static int Count() {
  int count=0;
  EnumWindows((h,p)=> {
   var t=new StringBuilder(80); var c=new StringBuilder(80);
   GetWindowText(h,t,80); GetClassName(h,c,80);
   if(IsWindowVisible(h)&&t.ToString()=="Run"&&c.ToString()=="#32770")count++;
   return true;
  },IntPtr.Zero);
  return count;
 }
}
'@
[ClassroomRunCheck]::Count() | Set-Content $Output
