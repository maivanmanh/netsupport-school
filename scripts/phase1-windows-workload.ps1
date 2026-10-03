param([int]$Seconds=60)
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$form=New-Object Windows.Forms.Form
$form.Text='Classroom Phase 1 repeatable workload'
$form.WindowState='Maximized'
$box=New-Object Windows.Forms.RichTextBox
$box.Dock='Fill'; $box.Font=New-Object Drawing.Font('Consolas',18)
$box.BackColor=[Drawing.Color]::Black; $box.ForeColor=[Drawing.Color]::White
$form.Controls.Add($box)
$start=[Diagnostics.Stopwatch]::StartNew()
$script:lastPhase=-1; $script:tick=0
$timer=New-Object Windows.Forms.Timer; $timer.Interval=50
$timer.Add_Tick({
    $elapsed=$start.Elapsed.TotalSeconds
    if($elapsed -ge $Seconds){$form.Close();return}
    $phase=[int][Math]::Floor($elapsed/10)%5
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
