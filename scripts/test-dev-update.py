#!/usr/bin/env python3
"""Isolated native Mac dev-update smoke: bad checksum, then full reconnect.

No capture/control injection, SSH, live build replacement or privacy changes.
An isolated Teacher still creates its native background host/tray.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
import zipfile

ROOT = Path(__file__).resolve().parent.parent
TOOL = ROOT / "scripts/classroom-dev-update.py"
spec = importlib.util.spec_from_file_location("dev_update_cli", TOOL)
cli = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cli)


def wait_for(test, processes, seconds=20):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        for process in processes:
            process.poll()  # reap original Student so updater can observe exit
        result = test()
        if result:
            return result
        time.sleep(0.1)
    raise TimeoutError("Isolated test did not reach the expected state")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--teacher", default=str(ROOT / "target/release/Classroom"))
    parser.add_argument("--student", default=str(ROOT / "target/release/ClassroomStudent"))
    parser.add_argument("--output", required=True)
    parser.add_argument("--port", type=int, default=49600)
    args = parser.parse_args()
    directory = Path(args.output).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    endpoint = directory / "teacher-endpoint.json"
    config = directory / "config.toml"
    config.write_text("[monitoring]\nenabled = false\n")
    source = Path(args.student).resolve()
    if source.is_dir():
        target = directory / source.name
        shutil.copytree(source, target, ignore=shutil.ignore_patterns("logs", ".DS_Store"))
        student_exe = target / "Contents/MacOS/ClassroomStudent"
    else:
        target = directory / "ClassroomStudent"
        shutil.copyfile(source, target)
        target.chmod(0o755)
        student_exe = target
    package = directory / "student-update.zip"
    subprocess.run([sys.executable, str(TOOL), "package", str(target), "--version", "smoke-new", "--output", str(package)], check=True)
    bad = directory / "bad-checksum.zip"
    with zipfile.ZipFile(package) as src, zipfile.ZipFile(bad, "w", zipfile.ZIP_DEFLATED) as dst:
        for item in src.infolist():
            data = src.read(item)
            if item.filename == "manifest.json":
                manifest = json.loads(data)
                manifest["sha256"] = "0" * 64
                data = json.dumps(manifest).encode()
            dst.writestr(item, data)
    handles = []
    processes = []
    client = None
    try:
        def start(command, name):
            output = open(directory / name, "wb")
            handles.append(output)
            process = subprocess.Popen(command, stdout=output, stderr=output, start_new_session=True)
            processes.append(process)
            return process
        teacher = start([args.teacher, "--role", "teacher", "--teacher-agent", "--bind-ip", "127.0.0.1",
                         "--control-port", str(args.port), "--ipc-file", str(endpoint), "--config", str(config),
                         "--launch-profile", str(directory / "teacher/profile.toml"), "--log-file", str(directory / "teacher.log")], "teacher.stdout")
        wait_for(endpoint.exists, processes)
        student = start([str(student_exe), "--teacher-ip", "127.0.0.1", "--control-port", str(args.port),
                         "--media-port", str(args.port + 10), "--feedback-port", str(args.port + 11), "--headless",
                         "--config", str(config), "--launch-profile", str(directory / "student/profile.toml"),
                         "--log-file", str(directory / "student.log")], "student.stdout")
        client = cli.IPC(endpoint)
        wait_for(lambda: (client.read(), client.state.get("tools", {}).get("ready"))[1], processes)
        student_id = client.state["tools"]["ready"][0]
        assert client.state["tools"]["capabilities"][student_id]["dev_update"] == 1
        initial_hash = cli.sha256(student_exe)
        def submit(path):
            staged = endpoint.parent / "outbox" / (path.stem + "-copy")
            shutil.copyfile(path, staged)
            return client.command("tool_update", [student_id], {"path": str(staged), "name": path.name})["job_id"]
        def job_result(job_id):
            client.read()
            job = next((j for j in client.state["tools"]["jobs"] if j["id"] == job_id), None)
            return job if job and job["targets"][0]["status"] in {"complete", "error", "cancelled"} else None
        submit_id = submit(bad)
        failed = wait_for(lambda: job_result(submit_id), processes, 60)
        assert failed["targets"][0]["status"] == "error", failed
        assert student.poll() is None and cli.sha256(student_exe) == initial_hash
        print("PASS: bad executable hash rejected; original Student remains live", flush=True)
        deployed = start([sys.executable, str(TOOL), "deploy", "--endpoint", str(endpoint),
                          "--package", str(package), "--all-compatible", "--timeout", "90"], "deploy.stdout")
        wait_for(lambda: deployed.poll() is not None, processes, 100)
        assert deployed.returncode == 0, (directory / "deploy.stdout").read_text()
        def final_job():
            client.read()
            return next((j for j in reversed(client.state["tools"]["jobs"]) if j["kind"] == "update" and j["targets"][0]["status"] == "complete"), None)
        completed = wait_for(final_job, processes)
        assert completed["targets"][0]["status"] == "complete", completed
        assert student.poll() == 0, "Original Student did not exit cleanly"
        assert cli.sha256(student_exe) == initial_hash
        assert client.state["tools"]["ready"].count(student_id) == 1
        backups = list(directory.glob(".classroom-update-*/previous"))
        assert len(backups) == 1 and backups[0].exists()
        print("PASS: update committed, old PID exited, replacement reconnected with same Student identity", flush=True)
        (directory / "REPORT.json").write_text(json.dumps({"bad_package": failed, "update": completed,
                                                         "old_pid": student.pid, "preserved_identity": student_id}, indent=2))
        client.command("quit")
    finally:
        if client:
            client.socket.close()
        # Kill only sessions created by this test; replacement and updater inherit
        # the original Student's group even when that original PID has exited.
        for process in reversed(processes):
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        for process in processes:
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
        for handle in handles:
            handle.close()


if __name__ == "__main__":
    main()
