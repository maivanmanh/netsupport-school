param([int]$Width=0,[int]$Height=0)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Windows.Forms
$device=[Windows.Forms.Screen]::PrimaryScreen.DeviceName
# DEVMODEW uses the display union. This test utility never writes registry modes.
if (-not ('ClassroomDisplay' -as [type])) { Add-Type @'
using System;
using System.Runtime.InteropServices;
public class ClassroomDisplay {
 [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
 public struct Mode {
  [MarshalAs(UnmanagedType.ByValTStr,SizeConst=32)] public string device;
  public ushort spec, driver, size, extra; public uint fields;
  public int x,y; public uint orientation,fixedOutput;
  public short color,duplex,yResolution,ttOption,collate;
  [MarshalAs(UnmanagedType.ByValTStr,SizeConst=32)] public string form;
  public ushort logPixels; public uint bits,width,height,flags,frequency;
  public uint icmMethod,icmIntent,mediaType,ditherType,reserved1,reserved2,panningWidth,panningHeight;
 }
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern bool EnumDisplaySettings(string device,int index,ref Mode mode);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int ChangeDisplaySettings(ref Mode mode,uint flags);
 public static Mode NewMode(){var m=new Mode();m.size=(ushort)Marshal.SizeOf(typeof(Mode));return m;}
}
'@
}
$current=[ClassroomDisplay]::NewMode()
if(-not [ClassroomDisplay]::EnumDisplaySettings($device,-1,[ref]$current)){throw "Current display mode unavailable on $device (DEVMODE size=$($current.size))"}
if($Width -eq 0){
 $modes=@();$i=0
 while($true){$m=[ClassroomDisplay]::NewMode();if(-not [ClassroomDisplay]::EnumDisplaySettings($device,$i,[ref]$m)){break};$modes+=@{width=$m.width;height=$m.height;bits=$m.bits;hz=$m.frequency};$i++}
 @{current=@{width=$current.width;height=$current.height;hz=$current.frequency};modes=$modes} | ConvertTo-Json -Depth 4
}else{
 $selected=$null;$i=0
 while($true){$m=[ClassroomDisplay]::NewMode();if(-not [ClassroomDisplay]::EnumDisplaySettings($device,$i,[ref]$m)){break};if($m.width -eq $Width -and $m.height -eq $Height -and $m.bits -eq $current.bits){$selected=$m;break};$i++}
 if($null -eq $selected){throw "Unsupported display mode ${Width}x${Height}"}
 if([ClassroomDisplay]::ChangeDisplaySettings([ref]$selected,2) -ne 0){throw 'Display mode test rejected'}
 if([ClassroomDisplay]::ChangeDisplaySettings([ref]$selected,0) -ne 0){throw 'Display mode switch rejected'}
 @{previousWidth=$current.width;previousHeight=$current.height;width=$Width;height=$Height}|ConvertTo-Json
}
