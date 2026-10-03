param([int]$ProcessId,[string]$Output)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class ClassroomWindow {
 [StructLayout(LayoutKind.Sequential)] public struct Rect {public int left,top,right,bottom;}
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out Rect rect);
 [DllImport("user32.dll",EntryPoint="GetWindowLongW")] public static extern int GetWindowLong(IntPtr hwnd,int index);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd,uint msg,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd,int command);
}
'@
$handle=[IntPtr]::Zero
for($i=0;$i -lt 100;$i++){
 $p=Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
 if(-not $p){throw 'Student ended before window audit'}
 $p.Refresh();$handle=$p.MainWindowHandle
 if($handle -ne [IntPtr]::Zero){break};Start-Sleep -Milliseconds 100
}
if($handle -eq [IntPtr]::Zero){throw 'No Student preview window'}
# The windowless console subsystem is excluded: MainWindowHandle must identify
# the renderer by its style (borderless/topmost), captured below.
$rect=New-Object ClassroomWindow+Rect
$null=[ClassroomWindow]::GetWindowRect($handle,[ref]$rect)
$screen=[Windows.Forms.Screen]::PrimaryScreen.Bounds
$style=[ClassroomWindow]::GetWindowLong($handle,-16)
$extended=[ClassroomWindow]::GetWindowLong($handle,-20)
$null=[ClassroomWindow]::PostMessage($handle,0x10,[IntPtr]::Zero,[IntPtr]::Zero)
Start-Sleep -Seconds 2
$alive=[ClassroomWindow]::IsWindow($handle)
if($alive){$null=[ClassroomWindow]::ShowWindow($handle,6);Start-Sleep -Seconds 2}
$after=New-Object ClassroomWindow+Rect
$null=[ClassroomWindow]::GetWindowRect($handle,[ref]$after)
@{handle=$handle.ToInt64();topmost=(($extended -band 8)-ne 0);borderless=(($style -band 0xC00000)-eq 0);width=$rect.right-$rect.left;height=$rect.bottom-$rect.top;screenWidth=$screen.Width;screenHeight=$screen.Height;survivedClose=$alive;restoredWidth=$after.right-$after.left;restoredHeight=$after.bottom-$after.top} | ConvertTo-Json | Set-Content $Output
if($alive){
 $bmp=New-Object Drawing.Bitmap($screen.Width,$screen.Height)
 $graphics=[Drawing.Graphics]::FromImage($bmp)
 $graphics.CopyFromScreen($screen.Location,[Drawing.Point]::Empty,$screen.Size)
 $bmp.Save([IO.Path]::ChangeExtension($Output,'.png'),[Drawing.Imaging.ImageFormat]::Png)
 $graphics.Dispose();$bmp.Dispose()
}
