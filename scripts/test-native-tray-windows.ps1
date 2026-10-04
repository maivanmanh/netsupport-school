param(
    [Parameter(Mandatory=$true)][int]$ProcessId,
    [ValidateSet('Exit','Restart','Settings','SaveWifi','Cursor','CursorRestore')][string]$Command='Exit',
    [string]$OutputFile
)
# Run in the interactive desktop. Targets only the specified live Classroom PID.
$ErrorActionPreference='Stop'
trap {
    if ($OutputFile) { [IO.File]::WriteAllText($OutputFile, (@{error=$_.ToString();process_id=$ProcessId;command=$Command}|ConvertTo-Json -Compress)) }
    throw $_
}
$process=Get-Process -Id $ProcessId
if ($process.ProcessName -notmatch '^Classroom(Agent|Student)?$') { throw 'Expected a Classroom Rust process' }
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class ClassroomDesktopProbe {
    delegate bool EnumProc(IntPtr hwnd, IntPtr parameter);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback,IntPtr parameter);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd,StringBuilder text,int capacity);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr hwnd,StringBuilder text,int capacity);
    [DllImport("user32.dll")] static extern bool PostMessage(IntPtr hwnd,uint message,IntPtr first,IntPtr second);
    [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr hwnd,uint message,IntPtr first,IntPtr second);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern bool AllowSetForegroundWindow(uint process);
    [DllImport("user32.dll")] static extern int GetMenuItemCount(IntPtr menu);
    [DllImport("user32.dll")] static extern uint GetMenuItemID(IntPtr menu,int position);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetMenuString(IntPtr menu,uint item,StringBuilder text,int capacity,uint flags);
    [DllImport("user32.dll")] static extern bool GetMenuItemRect(IntPtr hwnd,IntPtr menu,uint item,out Rect rect);
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
    [DllImport("user32.dll")] static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
    [DllImport("user32.dll")] static extern bool GetCursorInfo(ref CursorInfo info);
    [StructLayout(LayoutKind.Sequential)] struct Point { public int x,y; }
    [StructLayout(LayoutKind.Sequential)] struct Rect { public int left,top,right,bottom; }
    [StructLayout(LayoutKind.Sequential)] struct CursorInfo { public uint size,flags; public IntPtr cursor; public int x,y; }
    static IntPtr Find(int pid,string name) {
        IntPtr found=IntPtr.Zero;
        EnumWindows((hwnd,param)=> {
            uint owner; GetWindowThreadProcessId(hwnd,out owner);
            var text=new StringBuilder(256); GetClassName(hwnd,text,256);
            if(owner==pid && text.ToString()==name) { found=hwnd; return false; } return true;
        },IntPtr.Zero);
        return found;
    }
    public static void Select(int pid,string label) {
        var tray=Find(pid,"tray_icon_app"); if(tray==IntPtr.Zero) throw new Exception("Tray missing");
        AllowSetForegroundWindow((uint)pid);
        SendMessage(tray,0x1f,IntPtr.Zero,IntPtr.Zero);
        PostMessage(tray,6002,IntPtr.Zero,new IntPtr(0x204));
        IntPtr popup=IntPtr.Zero;
        for(int i=0;i<100 && popup==IntPtr.Zero;i++) { System.Threading.Thread.Sleep(20); popup=Find(pid,"#32768"); }
        if(popup==IntPtr.Zero) throw new Exception("Native popup missing");
        var menu=SendMessage(popup,0x01e1,IntPtr.Zero,IntPtr.Zero);
        for(uint i=0;i<GetMenuItemCount(menu);i++) {
            var text=new StringBuilder(256); GetMenuString(menu,i,text,256,0x400);
            if(text.ToString()!=label) continue;
            Rect rect; if(!GetMenuItemRect(tray,menu,i,out rect)) throw new Exception("Menu bounds unavailable");
            // Deliver the native command Windows emits for this exact menu item.
            // No global click can reach another application if focus changes.
            var id=GetMenuItemID(menu,(int)i);
            SendMessage(tray,0x1f,IntPtr.Zero,IntPtr.Zero);
            PostMessage(tray,0x111,new IntPtr(id),IntPtr.Zero);
            return;
        }
        throw new Exception("Menu item missing: "+label);
    }
    public static uint Cursor(int pid,bool restore) {
        if(!restore) {
            var surface=Find(pid,"ClassroomStudentPresentation");
            if(surface==IntPtr.Zero) throw new Exception("Presentation surface missing");
            SetCursorPos(310,310);
            SendMessage(surface,0x20,surface,new IntPtr(1));
        } else { SetCursorPos(311,311); }
        System.Threading.Thread.Sleep(150);
        var info=new CursorInfo(); info.size=(uint)Marshal.SizeOf(typeof(CursorInfo));
        if(!GetCursorInfo(ref info)) throw new Exception("Cursor query failed"); return info.flags;
    }
    [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr hwnd,ref Point point);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr hwnd,int mode);
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point point);
    static void Click(IntPtr hwnd,int x,int y) {
        ShowWindow(hwnd,9);SetForegroundWindow(hwnd);System.Threading.Thread.Sleep(200);
        var point=new Point();point.x=x;point.y=y;ClientToScreen(hwnd,ref point);SetCursorPos(point.x,point.y);
        System.Threading.Thread.Sleep(100);
        uint intended,actual;GetWindowThreadProcessId(hwnd,out intended);GetWindowThreadProcessId(WindowFromPoint(point),out actual);
        if(intended!=actual)throw new Exception("Student settings not under cursor");
        mouse_event(2,0,0,0,UIntPtr.Zero);
        System.Threading.Thread.Sleep(120);
        mouse_event(4,0,0,0,UIntPtr.Zero);
        System.Threading.Thread.Sleep(180);
    }
    public static void SaveWifi(int pid) {
        IntPtr settings=IntPtr.Zero;
        EnumWindows((hwnd,param)=> {
            uint owner; GetWindowThreadProcessId(hwnd,out owner);
            var text=new StringBuilder(256); GetWindowText(hwnd,text,256);
            if(owner==pid && text.ToString()=="Classroom Student | Settings") { settings=hwnd;return false; } return true;
        },IntPtr.Zero);
        if(settings==IntPtr.Zero) throw new Exception("Student settings window missing");
        Click(settings,70,185); // Presentation sidebar
        Click(settings,620,371); // Wi-Fi preset
        Click(settings,700,550); // Save & restart
    }
}
'@
$result=@{ process_id=$ProcessId; command=$Command; sampled_utc=[DateTime]::UtcNow.ToString('o') }
if ($Command -eq 'SaveWifi') {
    [ClassroomDesktopProbe]::SaveWifi($ProcessId)
} elseif ($Command -in @('Cursor','CursorRestore')) {
    $result.cursor_flags=[ClassroomDesktopProbe]::Cursor($ProcessId, $Command -eq 'CursorRestore')
} else {
    $label=@{Exit='Exit Classroom'; Restart='Restart Classroom'; Settings='Settings...'}[$Command]
    [ClassroomDesktopProbe]::Select($ProcessId,$label)
    if ($Command -eq 'Exit') { $result.process_exited=$process.WaitForExit(10000) }
}
$json=$result|ConvertTo-Json -Compress
if ($OutputFile) { [IO.File]::WriteAllText($OutputFile,$json) }
$json
