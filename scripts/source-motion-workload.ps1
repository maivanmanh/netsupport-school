param([int]$Seconds=140,[string]$EvidencePath='')
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class ClassroomMotionForeground {
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
}
'@
$screen=[Windows.Forms.Screen]::PrimaryScreen.Bounds
$forms=@();$boxes=@()
for($i=0;$i -lt 2;$i++) {
 $f=New-Object Windows.Forms.Form
 $f.Text="Classroom source measurement $($i+1)"
 $f.StartPosition='Manual';$f.Left=$screen.X+40;$f.Top=$screen.Y+40
 $f.Width=[Math]::Min(1200,$screen.Width-80);$f.Height=[Math]::Min(700,$screen.Height-80)
 $b=New-Object Windows.Forms.RichTextBox
 $b.Dock='Fill';$b.ReadOnly=$true;$b.Font=New-Object Drawing.Font('Consolas',18,[Drawing.FontStyle]::Regular,[Drawing.GraphicsUnit]::Pixel)
 $b.BackColor=[Drawing.Color]::Black;$b.ForeColor=[Drawing.Color]::White
 $f.Controls.Add($b);$forms+=,$f;$boxes+=,$b
}
$code=(0..239 | ForEach-Object {"$($_): fn render_frame() { latest_frame(); } // Tiếng Việt"}) -join "`n"
$boxes[0].Text=$code;$boxes[1].Text="SECOND WINDOW`n"+$code
$forms[1].Show();$forms[0].Show();$forms[0].Activate()
$start=[Diagnostics.Stopwatch]::StartNew()
$script:previous=-1;$script:tick=0;$script:lastAudit=-1;$script:active=0;$script:updates=0;$script:nextUpdate=0.0
$timer=New-Object Windows.Forms.Timer;$timer.Interval=15
$timer.Add_Tick({
 $elapsed=$start.Elapsed.TotalSeconds
 if($elapsed -ge $Seconds){$forms[1].Close();$forms[0].Close();return}
 if($elapsed -lt $script:nextUpdate){return}
 $phase=[int][Math]::Floor($elapsed/10)%7
 $script:tick=[int][Math]::Floor($elapsed/0.05)+1;$script:nextUpdate=$script:tick*0.05;$script:updates++
 if($phase -ne $script:previous) {
  $script:active=0;$forms[0].Left=$screen.X+40;$forms[0].Top=$screen.Y+40
  $forms[0].Activate();[ClassroomMotionForeground]::SetForegroundWindow($forms[0].Handle)|Out-Null
  $boxes[0].Text=if($phase -eq 2){$code}else{"STATIC: readable code / terminal / Tiếng Việt`n"+(("fn main() { latest_frame(); }`n")*25)}
  $boxes[0].BackColor=[Drawing.Color]::Black;$script:previous=$phase
 }
 switch($phase) {
  1 {$boxes[0].AppendText("let frame_$script:tick = latest_frame();`n");$boxes[0].ScrollToCaret()}
  2 {$boxes[0].SelectionStart=[Math]::Min($code.Length-1,($script:tick*55)%[Math]::Max(1,$code.Length-300));$boxes[0].SelectionLength=0;$boxes[0].ScrollToCaret()}
  3 {$boxes[0].AppendText("[$('{0:F3}' -f $elapsed)] INFO bounded latest-frame tick=$script:tick`n");$boxes[0].ScrollToCaret()}
  4 {$boxes[0].BackColor=[Drawing.Color]::FromArgb(($script:tick*17)%256,($script:tick*31)%256,($script:tick*7)%256);$boxes[0].Text="MOTION STRESS $script:tick`n"+('Moving color and text '*150)}
  5 {$forms[0].Left=$screen.X+40+($script:tick%20)*2;$forms[0].Top=$screen.Y+40+($script:tick%16)*2}
  6 {if($script:tick%20 -eq 0){$script:active=1-$script:active;$forms[$script:active].Activate();[ClassroomMotionForeground]::SetForegroundWindow($forms[$script:active].Handle)|Out-Null}}
 }
 if([int][Math]::Floor($elapsed) -ne $script:lastAudit) {
  $script:lastAudit=[int][Math]::Floor($elapsed)
  $foreground=[ClassroomMotionForeground]::GetForegroundWindow() -eq $forms[$script:active].Handle
  if($EvidencePath){@{unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds();elapsed_s=$elapsed;phase=$phase;tick=$script:tick;updates=$script:updates;scroll_anchor=$boxes[0].GetCharIndexFromPosition((New-Object Drawing.Point(1,1)));foreground=$foreground;visible=$forms[$script:active].Visible;screen_width=$screen.Width;screen_height=$screen.Height;active_window=$script:active}|ConvertTo-Json -Compress|Add-Content -Encoding UTF8 $EvidencePath}
  if(-not $foreground){$forms[$script:active].Activate();[ClassroomMotionForeground]::SetForegroundWindow($forms[$script:active].Handle)|Out-Null}
 }
})
try {$timer.Start();[Windows.Forms.Application]::Run($forms[0])}
finally {$timer.Dispose();foreach($f in $forms){$f.Dispose()}}
