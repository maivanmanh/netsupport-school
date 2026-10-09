param([int]$Seconds=60,[int]$Width=0,[int]$Height=0,[string]$EvidencePath='', [string]$StopFile='')
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class ClassroomForeground {
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
}
'@
$original=(& "$PSScriptRoot/phase1-display-modes.ps1" | ConvertFrom-Json).current
function Write-Evidence($event) {
 if($EvidencePath){$event | ConvertTo-Json -Compress | Add-Content -Encoding UTF8 $EvidencePath}
}
try {
if($Width -gt 0){& "$PSScriptRoot/phase1-display-modes.ps1" -Width $Width -Height $Height | Out-Null}
$form=New-Object Windows.Forms.Form
$form.Text='Classroom Phase 1 repeatable workload'
$form.WindowState='Maximized'
$box=New-Object Windows.Forms.RichTextBox
$box.Dock='Fill'; $box.Font=New-Object Drawing.Font('Consolas',18)
$box.BackColor=[Drawing.Color]::Black; $box.ForeColor=[Drawing.Color]::White
$form.Controls.Add($box)
$form.KeyPreview=$true
$form.Add_KeyDown({
 if($_.KeyCode -eq [Windows.Forms.Keys]::Home){
   Write-Evidence @{event='teacher_home';utc=[DateTime]::UtcNow.ToString('o');pid=$PID}
 }
})
$start=[Diagnostics.Stopwatch]::StartNew()
$script:lastPhase=-1; $script:tick=0
$script:lastEvidence=-1
$form.Add_Shown({$form.Activate();[ClassroomForeground]::SetForegroundWindow($form.Handle) | Out-Null})
$timer=New-Object Windows.Forms.Timer; $timer.Interval=50
$timer.Add_Tick({
    $elapsed=$start.Elapsed.TotalSeconds
    if($elapsed -ge $Seconds -or ($StopFile -and (Test-Path $StopFile))){$form.Close();return}
    $phase=[int][Math]::Floor($elapsed/10)%5
    if([int][Math]::Floor($elapsed) -ne $script:lastEvidence){
      $script:lastEvidence=[int][Math]::Floor($elapsed)
      $mode=(& "$PSScriptRoot/phase1-display-modes.ps1" | ConvertFrom-Json).current
      Write-Evidence @{event='sample';utc=[DateTime]::UtcNow.ToString('o');elapsed=$elapsed;phase=$phase;pid=$PID;width=$mode.width;height=$mode.height;foreground=([ClassroomForeground]::GetForegroundWindow() -eq $form.Handle)}
    }
    $script:tick++
    if($phase -ne $script:lastPhase){$box.Clear();$script:lastPhase=$phase}
    switch($phase){
      0 { if($box.TextLength -eq 0){$box.Text="STATIC: readable code / terminal / accents Tiếng Việt`r`n" + (('fn main() { println!("Classroom 1080p"); }' + "`r`n")*25)} }
      1 { $box.AppendText("let frame_$script:tick = latest_frame();`r`n");$box.ScrollToCaret() }
      2 { $box.Text=(0..45 | ForEach-Object {"$($script:tick+$_): scrolling source / freshness / RTP H.264"}) -join "`r`n" }
      3 { $box.AppendText("[$('{0:F3}' -f $elapsed)] INFO bounded latest-frame slot tick=$script:tick`r`n");$box.ScrollToCaret() }
      4 { $box.BackColor=[Drawing.Color]::FromArgb(($script:tick*17)%256,($script:tick*31)%256,($script:tick*7)%256);$box.Text="MOTION STRESS $elapsed`r`n"+('Moving full-screen color and text '*150) }
    }
    if($phase -ne 4){$box.BackColor=[Drawing.Color]::Black}
})
$timer.Start()
[Windows.Forms.Application]::Run($form)
$timer.Dispose();$form.Dispose()
} finally {
 if($Width -gt 0){
   & "$PSScriptRoot/phase1-display-modes.ps1" -Width $original.width -Height $original.height | Out-Null
   Write-Evidence @{event='restored';width=$original.width;height=$original.height;utc=[DateTime]::UtcNow.ToString('o')}
 }
}
