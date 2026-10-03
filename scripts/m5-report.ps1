param(
    [Parameter(Mandatory = $true)][string]$LogsDirectory,
    [Parameter(Mandatory = $true)][ValidateRange(1, 40)][int]$ExpectedStudents
)

$ErrorActionPreference = 'Stop'
$culture = [System.Globalization.CultureInfo]::InvariantCulture

function Read-MetricLines([string]$path) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing log: $path" }
    $text = [System.IO.File]::ReadAllText((Resolve-Path -LiteralPath $path).Path)
    $text = [regex]::Replace($text, ([string][char]27 + '\[[0-9;]*m'), '')
    return @($text -split "`r?`n")
}

function Get-Fields([string]$line) {
    $fields = @{}
    foreach ($match in [regex]::Matches($line, '([a-z_][a-z0-9_]*)=([^\s]+)')) {
        $fields[$match.Groups[1].Value] = $match.Groups[2].Value
    }
    return $fields
}

function Get-FinalFields([string[]]$lines, [string]$marker) {
    $matchingLines = @($lines | Where-Object { $_.Contains($marker) })
    if ($matchingLines.Count -eq 0) { throw "Missing final marker: $marker" }
    return (Get-Fields $matchingLines[-1])
}

function Number($fields, [string]$key) {
    if (-not $fields.ContainsKey($key)) { throw "Missing metric: $key" }
    return [double]::Parse($fields[$key], $culture)
}

function OptionalNumber($fields, [string]$key) {
    if (-not $fields.ContainsKey($key)) { return $null }
    return Number $fields $key
}

$teacherLines = Read-MetricLines (Join-Path $LogsDirectory 'teacher.log')
$teacherFinal = Get-FinalFields $teacherLines 'teacher M3 media complete'
$scaleIntervals = @(
    $teacherLines |
        Where-Object { $_.Contains('teacher M3 media interval') } |
        ForEach-Object { Get-Fields $_ } |
        Where-Object { (Number $_ 'connected_students') -eq $ExpectedStudents }
)
if ($scaleIntervals.Count -eq 0) {
    throw "Teacher log has no complete interval with $ExpectedStudents connected Students"
}
if ((Number $teacherFinal 'encoder_instances') -ne 1) { throw 'Teacher used more than one encoder' }

$studentFiles = @(Get-ChildItem -LiteralPath $LogsDirectory -Filter 'student-*.log' -File | Sort-Object Name)
if ($studentFiles.Count -ne $ExpectedStudents) {
    throw "Expected $ExpectedStudents student logs, found $($studentFiles.Count)"
}
$students = @(
    foreach ($studentFile in $studentFiles) {
        $final = Get-FinalFields (Read-MetricLines $studentFile.FullName) 'student M3 media complete'
        $rttSamples = OptionalNumber $final 'control_rtt_samples'
        $rttMs = $null
        if ($rttSamples -gt 0) { $rttMs = OptionalNumber $final 'control_rtt_ms' }
        [pscustomobject]@{
            Log = $studentFile.Name
            ReceiveMbps = Number $final 'receive_mbps'
            MissingPackets = Number $final 'missing_packets'
            NacksSent = Number $final 'nacks_sent'
            DecodeFps = Number $final 'decode_fps'
            RenderFps = Number $final 'render_fps'
            DecodeErrors = Number $final 'decode_errors'
            CpuPercent = Number $final 'process_cpu_percent'
            WorkingSetMiB = (Number $final 'process_working_set_bytes') / 1MB
            ControlRttSamples = $rttSamples
            ControlRttLastMs = $rttMs
        }
    }
)

function MeanMetric([string]$key) {
    $values = @($scaleIntervals | ForEach-Object { Number $_ $key })
    return ($values | Measure-Object -Average).Average
}

function MeanOptionalMetric([string]$key) {
    if (@($scaleIntervals | Where-Object { -not $_.ContainsKey($key) }).Count -gt 0) {
        return $null
    }
    return MeanMetric $key
}

$summary = [pscustomobject]@{
    ExpectedStudents = $ExpectedStudents
    FullScaleIntervals = $scaleIntervals.Count
    EncoderInstances = Number $teacherFinal 'encoder_instances'
    TeacherCaptureFpsMean = MeanMetric 'capture_fps'
    TeacherEncodeFpsMean = MeanMetric 'encode_fps'
    TeacherCpuPercentMean = MeanMetric 'process_cpu_percent'
    TeacherWorkingSetMiBPeak = (@($scaleIntervals | ForEach-Object { (Number $_ 'process_working_set_bytes') / 1MB }) | Measure-Object -Maximum).Maximum
    TeacherSendMbpsMean = MeanMetric 'send_mbps'
    TeacherSlowStudentsMean = MeanOptionalMetric 'receiver_slow_students'
    TeacherEncodeP95MsOverall = Number $teacherFinal 'encode_latency_p95_ms'
    TeacherWaitingFramesReplacedOverall = Number $teacherFinal 'waiting_frames_replaced'
    TeacherSendErrorsOverall = OptionalNumber $teacherFinal 'send_errors'
    WorstStudentMissingPackets = ($students.MissingPackets | Measure-Object -Maximum).Maximum
    SlowestStudentDecodeFps = ($students.DecodeFps | Measure-Object -Minimum).Minimum
    SlowestStudentRenderFps = ($students.RenderFps | Measure-Object -Minimum).Minimum
    StudentsWithDecodeErrors = @($students | Where-Object { $_.DecodeErrors -gt 0 }).Count
}

$summaryPath = Join-Path $LogsDirectory 'm5-summary.csv'
$studentsPath = Join-Path $LogsDirectory 'm5-students.csv'
$summary | Export-Csv -LiteralPath $summaryPath -NoTypeInformation -Encoding UTF8
$students | Export-Csv -LiteralPath $studentsPath -NoTypeInformation -Encoding UTF8
$summary | Format-List
Write-Output "Reports: $summaryPath; $studentsPath"
