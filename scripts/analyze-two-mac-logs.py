#!/usr/bin/env python3
"""Dependency-free summary for logs produced by run-two-mac-test.sh."""
from __future__ import annotations
import re
import sys
import json
from pathlib import Path

FIELDS = ("capture_fps", "encode_fps", "decode_fps", "render_fps", "send_mbps",
          "receive_mbps", "missing_packets", "nacks_sent", "nack_sequences_sent",
          "repaired_gaps", "recovery_episodes", "recovery_peak_ms", "jitter_queue_depth",
          "render_gap_interval_p95_ms", "decoded_frame_wait_interval_p95_ms",
          "decoded_frames", "rendered_frames", "decode_errors", "sent_packets",
          "control_rtt_ms", "ingress_dropped_packets")

def values(text: str, field: str) -> list[float]:
    result: list[float] = []
    for raw in re.findall(rf"\b{re.escape(field)}=([^\s]+)", text):
        raw = raw.strip('"')
        if raw in {"None", "null", "?"}:
            continue
        try:
            result.append(float(raw.rstrip(',')))
        except ValueError:
            continue
    return result

def summarize(path: Path) -> None:
    text = path.read_text(errors="replace")
    print(f"[{path.name}] lines={len(text.splitlines())} bytes={len(text.encode())}")
    for field in FIELDS:
        observed = values(text, field)
        if observed:
            print(f"  {field}: last={observed[-1]:.3f} max={max(observed):.3f} samples={len(observed)}")
    for marker in ("ERROR", "WARN"):
        count = sum(bool(re.search(rf"\b{marker}\s", line)) for line in text.splitlines())
        if count:
            print(f"  events.{marker.lower()}={count}")

def has_connected_student(path: Path) -> bool:
    return bool(re.search(r"connected_students=[1-9][0-9]*", path.read_text(errors="replace")))

def stage_report(text: str) -> dict:
    result = {}
    for line in text.splitlines():
        if "media stage interval" not in line:
            continue
        row = dict(re.findall(r"(\w+)=([^\s]+)", line))
        name = row.get("stage")
        if not name:
            continue
        item = result.setdefault(name, {"samples": 0, "total_us": 0, "max_us": 0, "interval_p95_max_us": 0})
        count = int(row["samples"])
        item["samples"] += count
        item["total_us"] += count * int(row["mean_us"])
        item["max_us"] = max(item["max_us"], int(row["max_us"]))
        item["interval_p95_max_us"] = max(item["interval_p95_max_us"], int(row["p95_us"]))
    for item in result.values():
        item["weighted_mean_us"] = item.pop("total_us") / max(item["samples"], 1)
    return result

def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {Path(sys.argv[0]).name} RUN_DIR", file=sys.stderr)
        return 2
    run_dir = Path(sys.argv[1])
    logs = sorted(p for p in run_dir.rglob("*.log") if not p.name.endswith(".stdout.log"))
    if not logs:
        print("No .log files found", file=sys.stderr)
        return 1
    print(f"run={run_dir}")
    for log in logs:
        summarize(log)
    passed = True
    report = {"run": str(run_dir), "scenarios": {}, "logs": {}}
    for log in logs:
        text = log.read_text(errors="replace")
        report["logs"][str(log.relative_to(run_dir))] = {
            "stages": stage_report(text),
            "complete": [dict(re.findall(r"(\w+)=([^\s]+)", line)) for line in text.splitlines() if "M3 media complete" in line],
        }
    teachers = [log for log in logs if log.name.endswith("-teacher.log")]
    if not teachers:
        print("status=MISSING_TEACHER_LOG")
        return 1
    for teacher in teachers:
        scenario = teacher.name.removesuffix("-teacher.log")
        student = run_dir / f"{scenario}-remote" / "student.log"
        status_file = run_dir / f"{scenario}.status"
        status = dict(line.split("=", 1) for line in status_file.read_text().splitlines()) if status_file.exists() else {}
        teacher_text = teacher.read_text(errors="replace")
        restart = run_dir / f"{scenario}-restart.log"
        if restart.exists():
            teacher_text += "\n" + restart.read_text(errors="replace")
        student_text = student.read_text(errors="replace") if student.exists() else ""
        healthy = (has_connected_student(teacher)
                   and "teacher M3 media complete" in teacher_text
                   and "student M3 media complete" in student_text
                   and max(values(teacher_text, "sent_packets"), default=0) > 0
                   and max(values(student_text, "decoded_frames"), default=0) > 0
                   and status.get("student_exit") == "0"
                   and status.get("teacher_exit") == "0"
                   and (status.get("headless") == "1"
                        or max(values(student_text, "rendered_frames"), default=0) > 0))
        if scenario == "reconnect":
            completions = [line for line in student_text.splitlines() if "student M3 media complete" in line]
            healthy &= (student_text.count("student presentation registration complete") >= 2
                        and student_text.count("student preview closed after Teacher disconnect") >= 1
                        and len(completions) >= 2
                        and all(max(values(line, "decoded_frames"), default=0) > 0 for line in completions)
                        and (status.get("headless") == "1" or student_text.count("student forced presentation window ready") >= 2)
                        and restart.exists() and "teacher M3 media complete" in restart.read_text())
        if scenario == "disconnect":
            healthy &= student_text.count("student preview closed after Teacher disconnect") >= 1
            healthy &= student_text.count("student presentation registration complete") == 1
        if scenario == "fanout3":
            receivers = sorted((run_dir / f"{scenario}-remote").glob("student*.log"))
            receivers = [p for p in receivers if not p.name.endswith(".stdout.log")]
            healthy &= (len(receivers) == 3
                        and max(values(teacher_text, "connected_students"), default=0) == 3
                        and max(values(teacher_text, "encoder_instances"), default=0) == 1)
            for receiver in receivers:
                text = receiver.read_text(errors="replace")
                healthy &= ("student M3 media complete" in text
                            and max(values(text, "decoded_frames"), default=0) > 0)
        report["scenarios"][scenario] = {"functional_ok": bool(healthy), "process_status": status}
        print(f"scenario={scenario} status={'MEDIA_OK' if healthy else 'FAILED'}")
        passed &= healthy
    print(f"status={'MEDIA_OK' if passed else 'FAILED'}")
    (run_dir / "metrics.json").write_text(json.dumps(report, indent=2))
    return 0 if passed else 1

if __name__ == "__main__":
    raise SystemExit(main())
