//! Offline, platform-independent M5 log analysis. Never infer performance or
//! image-quality acceptance from child exit codes or packet counts alone.
use std::{collections::BTreeMap, fmt::Write as _, fs, path::Path};

use anyhow::{ensure, Context, Result};

const MAX_LOG_BYTES: u64 = 64 * 1024 * 1024;
type Fields = BTreeMap<String, f64>;

struct Log {
    intervals: Vec<Fields>,
    completed: Fields,
    headless: bool,
}

struct Report {
    summary: Vec<(&'static str, String)>,
    students: Vec<Vec<String>>,
}

pub fn generate(directory: &Path, expected: u16) -> Result<()> {
    ensure!(
        (1..=40).contains(&expected),
        "expected Students must be in 1..=40"
    );
    let teacher = parse(&read_log(&directory.join("teacher.log"))?, "teacher")?;
    let mut paths = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("student-"))
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("log"))
    });
    paths.sort();
    ensure!(
        paths.len() == usize::from(expected),
        "expected {expected} Student logs, found {}",
        paths.len()
    );
    let mut students = Vec::new();
    for path in paths {
        let log = parse(&read_log(&path)?, "student")
            .with_context(|| format!("invalid Student log {}", path.display()))?;
        students.push((
            path.file_name()
                .context("missing log filename")?
                .to_string_lossy()
                .into_owned(),
            log,
        ));
    }
    let report = summarize(&teacher, &students, expected)?;
    let mut summary = csv_row(report.summary.iter().map(|(key, _)| *key));
    summary.push_str(&csv_row(
        report.summary.iter().map(|(_, value)| value.as_str()),
    ));
    let mut student_csv = csv_row([
        "Log",
        "Mode",
        "ElapsedSeconds",
        "DecodedFrames",
        "RenderedFrames",
        "DecodeFpsOverall",
        "RenderFpsOverall",
        "ReceiveMbpsOverall",
        "MissingPackets",
        "NacksSent",
        "DecodeErrors",
        "CpuPercentFinalSample",
        "WorkingSetMiBFinalSample",
        "DecodeP95MsOverall",
        "RenderP95MsOverall",
        "ReceivedPackets",
        "KeyframeRequests",
        "PreKeyframeFramesDiscarded",
        "RenderSlotReplacements",
        "RenderGapP95Ms",
        "JitterDelayMs",
        "ControlRttSamples",
        "ControlRttLastMs",
        "Health",
    ]);
    for row in &report.students {
        student_csv.push_str(&csv_row(row.iter().map(String::as_str)));
    }
    // Only generated report files are replaced; original logs are never changed.
    fs::write(directory.join("m5-summary.csv"), summary)?;
    fs::write(directory.join("m5-students.csv"), student_csv)?;
    println!("Reports: {}", directory.canonicalize()?.display());
    for (key, value) in &report.summary {
        println!("{key}={value}");
    }
    Ok(())
}

fn read_log(path: &Path) -> Result<String> {
    let metadata = fs::metadata(path).with_context(|| format!("missing log {}", path.display()))?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_LOG_BYTES,
        "expected a regular log file no larger than 64 MiB: {}",
        path.display()
    );
    fs::read_to_string(path).with_context(|| format!("cannot read UTF-8 log {}", path.display()))
}

fn parse(text: &str, role: &str) -> Result<Log> {
    ensure!(
        !text.contains("COMPACTED"),
        "compacted logs cannot supply exact interval measurements; keep original logs"
    );
    let text = strip_ansi(text);
    let interval_marker = format!("{role} M3 media interval");
    let final_marker = format!("{role} M3 media complete");
    let mut intervals = Vec::new();
    let mut completed = None;
    let mut registrations = 0;
    for line in text.lines() {
        if line.contains("student presentation registration complete") {
            registrations += 1;
        }
        if line.contains(&interval_marker) {
            intervals.push(fields(line));
        }
        if line.contains(&final_marker) {
            ensure!(
                completed.is_none(),
                "multiple completed {role} sessions; use a fresh log per run"
            );
            completed = Some(fields(line));
        }
    }
    ensure!(
        registrations <= 1,
        "reconnected Student logs require separate session analysis"
    );
    Ok(Log {
        intervals,
        completed: completed
            .with_context(|| format!("missing {final_marker}; run may be incomplete"))?,
        headless: text.lines().any(|line| {
            line.contains("student M3 UDP receiver ready") && line.contains("headless=true")
        }),
    })
}

fn fields(line: &str) -> Fields {
    line.split_whitespace()
        .filter_map(|token| {
            let (key, value) = token.split_once('=')?;
            let parsed = match value {
                "true" => 1.0,
                "false" => 0.0,
                _ => value.parse().ok()?,
            };
            Some((key.to_owned(), parsed))
        })
        .collect()
}

fn metric(fields: &Fields, name: &str) -> Result<f64> {
    let value = *fields
        .get(name)
        .with_context(|| format!("missing numeric metric {name}; use current, complete logs"))?;
    ensure!(
        value.is_finite() && value >= 0.0,
        "invalid metric {name}={value}"
    );
    Ok(value)
}

fn optional_metric(fields: &Fields, name: &str) -> Result<Option<f64>> {
    fields.get(name).map(|_| metric(fields, name)).transpose()
}

fn optional_weighted(scale: &[&Fields], key: &str, seconds: f64) -> Result<Option<f64>> {
    let present = scale.iter().filter(|row| row.contains_key(key)).count();
    ensure!(
        present == 0 || present == scale.len(),
        "partial interval metric {key}; do not mix log versions within one run"
    );
    if present == 0 {
        return Ok(None);
    }
    let mut weighted = 0.0;
    for row in scale {
        weighted += metric(row, key)? * metric(row, "interval_seconds")?;
    }
    Ok(Some(weighted / seconds))
}

fn optional_number(value: Option<f64>) -> String {
    value.map_or_else(|| "not_recorded".to_owned(), number)
}

// These equality checks are for small integral endpoint/encoder counts,
// exactly representable as f64, not approximate performance measurements.
#[allow(clippy::too_many_lines, clippy::float_cmp)]
fn summarize(teacher: &Log, students: &[(String, Log)], expected: u16) -> Result<Report> {
    ensure!(
        students.len() == usize::from(expected),
        "Student log count mismatch"
    );
    ensure!(
        metric(&teacher.completed, "encoder_instances")? == 1.0,
        "Teacher must have exactly one encoder"
    );
    let mut scale = Vec::new();
    for row in &teacher.intervals {
        ensure!(
            metric(row, "encoder_instances")? == 1.0,
            "Teacher encoder count changed during the run"
        );
        // Requiring min == max excludes intervals spanning a join/disconnect,
        // even when the count happens to match at the interval's final instant.
        let min = metric(row, "connected_students_min")?;
        let max = metric(row, "connected_students_max")?;
        let end = metric(row, "connected_students")?;
        ensure!(
            min <= end && end <= max,
            "inconsistent interval Student counts"
        );
        if let Some(reporting) = optional_metric(row, "receiver_reporting_students")? {
            ensure!(
                reporting <= end,
                "more receiver reports than connected Students"
            );
            if let Some(high_loss) = optional_metric(row, "receiver_high_loss_students")? {
                ensure!(
                    high_loss <= reporting,
                    "high-loss Students exceed reporting Students"
                );
            }
            if let Some(slow) = optional_metric(row, "receiver_slow_students")? {
                ensure!(slow <= reporting, "slow Students exceed reporting Students");
            }
        }
        if let (Some(recovery), Some(feedback)) = (
            optional_metric(row, "receiver_recovery_eligible")?,
            optional_metric(row, "receiver_feedback_eligible")?,
        ) {
            ensure!(
                recovery <= feedback && feedback <= 1.0,
                "inconsistent adaptive feedback eligibility"
            );
        }
        if min == f64::from(expected) && max == f64::from(expected) {
            ensure!(
                metric(row, "interval_seconds")? > 0.0,
                "interval duration must be positive"
            );
            scale.push(row);
        }
    }
    ensure!(
        !scale.is_empty(),
        "no complete interval with {expected} Students connected throughout"
    );
    let seconds = scale
        .iter()
        .map(|row| metric(row, "interval_seconds"))
        .collect::<Result<Vec<_>>>()?
        .iter()
        .sum::<f64>();
    let mut summary = vec![
        (
            "EvidenceScope",
            "logs-only; headless does not verify rendering; LAN/E2E acceptance pending".to_owned(),
        ),
        (
            "EndToEndLatencyEvidence",
            "not_measured_by_process_logs".to_owned(),
        ),
        ("ExpectedStudents", expected.to_string()),
        ("FullScaleIntervals", scale.len().to_string()),
        ("FullScaleSeconds", number(seconds)),
        ("EncoderInstances", "1".to_owned()),
    ];
    for (name, key) in [
        ("TeacherCaptureFpsWeighted", "capture_fps"),
        ("TeacherEncodeFpsWeighted", "encode_fps"),
        ("TeacherCpuPercentWeighted", "process_cpu_percent"),
        ("TeacherSendMbpsWeighted", "send_mbps"),
    ] {
        let mut weighted = 0.0;
        for row in &scale {
            weighted += metric(row, key)? * metric(row, "interval_seconds")?;
        }
        summary.push((name, number(weighted / seconds)));
    }
    for (name, key, divisor) in [
        ("TeacherTargetFpsWeighted", "target_fps", 1.0),
        (
            "TeacherTargetBitrateMbpsWeighted",
            "target_bitrate_bps",
            1_000_000.0,
        ),
        (
            "TeacherReportingStudentsWeighted",
            "receiver_reporting_students",
            1.0,
        ),
        (
            "TeacherHighLossStudentsWeighted",
            "receiver_high_loss_students",
            1.0,
        ),
        ("TeacherSlowStudentsWeighted", "receiver_slow_students", 1.0),
        (
            "TeacherFeedbackEligiblePercentOfScaleTime",
            "receiver_feedback_eligible",
            0.01,
        ),
        (
            "TeacherRecoveryEligiblePercentOfScaleTime",
            "receiver_recovery_eligible",
            0.01,
        ),
    ] {
        summary.push((
            name,
            optional_number(optional_weighted(&scale, key, seconds)?.map(|value| value / divisor)),
        ));
    }
    let mut peak_memory = 0.0_f64;
    for row in &scale {
        peak_memory = peak_memory.max(metric(row, "process_working_set_bytes")?);
    }
    summary.push((
        "TeacherWorkingSetMiBPeakAtScale",
        number(peak_memory / 1_048_576.0),
    ));
    for (name, key) in [
        ("TeacherEncodeP95MsOverall", "encode_latency_p95_ms"),
        (
            "TeacherWaitingFramesReplacedOverall",
            "waiting_frames_replaced",
        ),
        (
            "TeacherRepeatedFrameSubmissionsOverall",
            "repeated_frame_submissions",
        ),
    ] {
        summary.push((name, number(metric(&teacher.completed, key)?)));
    }
    for (name, key, divisor) in [
        ("TeacherCapturedFrames", "captured_frames", 1.0),
        ("TeacherEncodedFrames", "encoded_frames", 1.0),
        (
            "TeacherCaptureLatencyP50MsOverall",
            "capture_latency_p50_ms",
            1.0,
        ),
        (
            "TeacherCaptureLatencyP95MsOverall",
            "capture_latency_p95_ms",
            1.0,
        ),
        (
            "TeacherEncodeLatencyP50MsOverall",
            "encode_latency_p50_ms",
            1.0,
        ),
        (
            "TeacherEncodedStreamMbpsOverall",
            "encoded_stream_mbps",
            1.0,
        ),
        (
            "TeacherAggregateSendMbpsOverall",
            "aggregate_send_mbps",
            1.0,
        ),
        (
            "TeacherFinalTargetBitrateMbps",
            "target_bitrate_bps",
            1_000_000.0,
        ),
        ("TeacherBitrateChanges", "bitrate_changes", 1.0),
        ("TeacherSendErrorsOverall", "send_errors", 1.0),
    ] {
        summary.push((
            name,
            optional_number(optional_metric(&teacher.completed, key)?.map(|value| value / divisor)),
        ));
    }
    let mut rows = Vec::new();
    let mut unhealthy = 0_u16;
    let mut slowest_decode = f64::INFINITY;
    for (filename, student) in students {
        let data = &student.completed;
        let decoded = metric(data, "decoded_frames")?;
        let rendered = metric(data, "rendered_frames")?;
        let missing = metric(data, "missing_packets")?;
        let errors = metric(data, "decode_errors")?;
        let decode_fps = metric(data, "decode_fps")?;
        let mode = if student.headless {
            "headless"
        } else {
            "visible-or-legacy"
        };
        let health = if decoded == 0.0 || (!student.headless && rendered == 0.0) {
            "no_frames"
        } else if missing > 0.0 || errors > 0.0 {
            "needs_attention"
        } else {
            "frames_received_no_reported_loss_or_decode_errors"
        };
        unhealthy += u16::from(health == "no_frames" || health == "needs_attention");
        slowest_decode = slowest_decode.min(decode_fps);
        let mut row = vec![filename.clone(), mode.to_owned()];
        for key in [
            "elapsed_seconds",
            "decoded_frames",
            "rendered_frames",
            "decode_fps",
            "render_fps",
            "receive_mbps",
            "missing_packets",
            "nacks_sent",
            "decode_errors",
            "process_cpu_percent",
        ] {
            row.push(number(metric(data, key)?));
        }
        row.push(number(
            metric(data, "process_working_set_bytes")? / 1_048_576.0,
        ));
        row.push(number(metric(data, "decode_latency_p95_ms")?));
        row.push(number(metric(data, "render_latency_p95_ms")?));
        for key in [
            "received_packets",
            "keyframe_requests",
            "pre_keyframe_frames_discarded",
            "render_slot_replacements",
            "render_gap_p95_ms",
            "jitter_delay_ms",
        ] {
            row.push(optional_number(optional_metric(data, key)?));
        }
        let rtt_samples = optional_metric(data, "control_rtt_samples")?;
        row.push(optional_number(rtt_samples));
        row.push(optional_number(if rtt_samples.unwrap_or(0.0) > 0.0 {
            optional_metric(data, "control_rtt_ms")?
        } else {
            None
        }));
        row.push(health.to_owned());
        rows.push(row);
    }
    summary.push(("SlowestStudentDecodeFpsOverall", number(slowest_decode)));
    summary.push(("StudentsNeedingAttention", unhealthy.to_string()));
    Ok(Report {
        summary,
        students: rows,
    })
}

fn number(value: f64) -> String {
    format!("{value:.6}")
}

fn csv_row<'a>(values: impl IntoIterator<Item = &'a str>) -> String {
    let mut row = String::new();
    for (index, value) in values.into_iter().enumerate() {
        if index > 0 {
            row.push(',');
        }
        let _ = write!(row, "\"{}\"", value.replace('"', "\"\""));
    }
    row.push('\n');
    row
}

fn strip_ansi(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        } else {
            clean.push(character);
        }
    }
    clean
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: &str = "teacher M3 media interval connected_students=1 connected_students_min=1 connected_students_max=1 encoder_instances=1 interval_seconds=1 capture_fps=10 encode_fps=10 process_cpu_percent=50 process_working_set_bytes=1048576 send_mbps=2";
    const FINAL: &str = "teacher M3 media complete encoder_instances=1 encode_latency_p95_ms=12 waiting_frames_replaced=3 repeated_frame_submissions=5";
    const STUDENT: &str = "student M3 UDP receiver ready headless=true\nstudent M3 media complete elapsed_seconds=5 decoded_frames=100 rendered_frames=0 decode_fps=20 render_fps=0 receive_mbps=2 missing_packets=0 nacks_sent=0 decode_errors=0 process_cpu_percent=30 process_working_set_bytes=1048576 decode_latency_p95_ms=5 render_latency_p95_ms=0";

    #[test]
    fn report_weights_durations_and_excludes_join_intervals() {
        let transition = INTERVAL
            .replace("connected_students_min=1", "connected_students_min=0")
            .replace("encode_fps=10", "encode_fps=1000");
        let slow = INTERVAL
            .replace("interval_seconds=1", "interval_seconds=3")
            .replace("encode_fps=10", "encode_fps=30");
        let teacher = parse(
            &format!("{transition}\n{INTERVAL}\n{slow}\n{FINAL}"),
            "teacher",
        )
        .unwrap();
        let report = summarize(
            &teacher,
            &[(
                "student-01.log".to_owned(),
                parse(STUDENT, "student").unwrap(),
            )],
            1,
        )
        .unwrap();
        assert_eq!(
            report
                .summary
                .iter()
                .find(|(key, _)| *key == "TeacherEncodeFpsWeighted")
                .unwrap()
                .1,
            "25.000000"
        );
        assert_eq!(
            report
                .summary
                .iter()
                .find(|(key, _)| *key == "FullScaleIntervals")
                .unwrap()
                .1,
            "2"
        );
        assert_eq!(
            report
                .summary
                .iter()
                .find(|(key, _)| *key == "StudentsNeedingAttention")
                .unwrap()
                .1,
            "0"
        );
    }

    #[test]
    fn rejects_compaction_missing_final_reconnect_and_extra_encoder() {
        assert!(parse(&format!("COMPACTED {INTERVAL}\n{FINAL}"), "teacher").is_err());
        assert!(parse(INTERVAL, "teacher").is_err());
        assert!(parse(&format!("{STUDENT}\nstudent presentation registration complete\nstudent presentation registration complete"), "student").is_err());
        let teacher = parse(
            &format!("{INTERVAL}\n{FINAL}").replace("encoder_instances=1", "encoder_instances=2"),
            "teacher",
        )
        .unwrap();
        assert!(summarize(
            &teacher,
            &[("s".to_owned(), parse(STUDENT, "student").unwrap())],
            1
        )
        .is_err());
    }

    #[test]
    fn flags_zero_decoded_frames_and_rejects_nonfinite_metrics() {
        let teacher = parse(&format!("{INTERVAL}\n{FINAL}"), "teacher").unwrap();
        let students = vec![(
            "s".to_owned(),
            parse(
                &STUDENT.replace("decoded_frames=100", "decoded_frames=0"),
                "student",
            )
            .unwrap(),
        )];
        assert_eq!(
            summarize(&teacher, &students, 1).unwrap().students[0]
                .last()
                .unwrap(),
            "no_frames"
        );
        assert!(metric(&fields("x=NaN"), "x").is_err());
        assert!(metric(&fields("x=-1"), "x").is_err());
        assert_eq!(strip_ansi("\u{1b}[32mhello\u{1b}[0m"), "hello");
        assert_eq!(csv_row(["a,b", "c\"d"]), "\"a,b\",\"c\"\"d\"\n");
    }

    #[test]
    fn reports_adaptive_feedback_and_required_media_metrics_when_recorded() {
        let first = format!(
            "{INTERVAL} target_fps=24 target_bitrate_bps=6000000 receiver_reporting_students=1 receiver_high_loss_students=0 receiver_slow_students=1 receiver_feedback_eligible=true receiver_recovery_eligible=true"
        );
        let second = format!(
            "{INTERVAL} target_fps=20 target_bitrate_bps=5100000 receiver_reporting_students=0 receiver_high_loss_students=0 receiver_slow_students=0 receiver_feedback_eligible=false receiver_recovery_eligible=false"
        );
        let final_line = format!(
            "{FINAL} captured_frames=60 encoded_frames=50 capture_latency_p50_ms=2 capture_latency_p95_ms=5 encode_latency_p50_ms=10 encoded_stream_mbps=4 aggregate_send_mbps=5 target_bitrate_bps=5100000 bitrate_changes=1"
        );
        let student_line = format!(
            "{STUDENT} received_packets=1000 keyframe_requests=1 pre_keyframe_frames_discarded=2 render_slot_replacements=3 render_gap_p95_ms=80 jitter_delay_ms=40 control_rtt_samples=5 control_rtt_ms=1.25"
        );
        let teacher = parse(&format!("{first}\n{second}\n{final_line}"), "teacher").unwrap();
        let report = summarize(
            &teacher,
            &[(
                "student-01.log".to_owned(),
                parse(&student_line, "student").unwrap(),
            )],
            1,
        )
        .unwrap();
        let student_fields = &report.students[0];
        assert_eq!(student_fields[student_fields.len() - 3], "5.000000");
        assert_eq!(student_fields[student_fields.len() - 2], "1.250000");
        let value = |key| {
            report
                .summary
                .iter()
                .find(|(name, _)| *name == key)
                .unwrap()
                .1
                .as_str()
        };
        assert_eq!(value("TeacherTargetFpsWeighted"), "22.000000");
        assert_eq!(value("TeacherTargetBitrateMbpsWeighted"), "5.550000");
        assert_eq!(value("TeacherReportingStudentsWeighted"), "0.500000");
        assert_eq!(value("TeacherSlowStudentsWeighted"), "0.500000");
        assert_eq!(
            value("TeacherFeedbackEligiblePercentOfScaleTime"),
            "50.000000"
        );
        assert_eq!(value("TeacherBitrateChanges"), "1.000000");
        assert_eq!(
            value("EndToEndLatencyEvidence"),
            "not_measured_by_process_logs"
        );
        assert!(report.students[0].contains(&"80.000000".to_owned()));
        assert!(report.students[0].contains(&"40.000000".to_owned()));
    }

    #[test]
    fn rejects_partial_or_impossible_adaptive_interval_telemetry() {
        let adaptive = format!("{INTERVAL} target_fps=24 receiver_reporting_students=1");
        let teacher = parse(&format!("{adaptive}\n{INTERVAL}\n{FINAL}"), "teacher").unwrap();
        let students = [(
            "student-01.log".to_owned(),
            parse(STUDENT, "student").unwrap(),
        )];
        assert!(summarize(&teacher, &students, 1).is_err());
        let impossible = format!("{INTERVAL} receiver_reporting_students=2");
        let teacher = parse(&format!("{impossible}\n{FINAL}"), "teacher").unwrap();
        assert!(summarize(&teacher, &students, 1).is_err());
    }
}
