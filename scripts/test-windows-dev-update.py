#!/usr/bin/env python3
"""Bootstrap Windows once via SSH, then update/confirm/collect logs via Teacher.

Passwords are prompted and retained only in memory. Run outside the workspace
sandbox for the existing password-SSH PTY helper. Uses an isolated Student/task
and ports; a bounded interactive worker cleans up its own processes/task later.
"""
import argparse
import getpass
import importlib.util
import ipaddress
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


remote_module = module("classroom_remote", ROOT / "scripts/run-four-machine-test.py")
cli = module("classroom_update", ROOT / "scripts/classroom-dev-update.py")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="mvmanh@192.168.50.94")
    parser.add_argument("--bootstrap", default=str(ROOT / "target/dev-update-build/ClassroomStudent-bootstrap.exe"))
    parser.add_argument("--replacement", default=str(ROOT / "target/rustup-windows/x86_64-pc-windows-gnu/release/ClassroomStudent.exe"))
    parser.add_argument("--replacement2", help="Second successive update, also through Classroom")
    parser.add_argument("--teacher", default=str(ROOT / "target/release/Classroom"))
    parser.add_argument("--output", required=True)
    parser.add_argument("--port", type=int, default=49700)
    parser.add_argument("--resume-bootstrap", help="Reuse files from an incomplete initial SSH setup; never recopy the exe")
    args = parser.parse_args()
    user, host = args.host.split("@", 1)
    if not re.fullmatch(r"[A-Za-z0-9_-]+", user):
        raise ValueError("Invalid Windows account")
    ipaddress.IPv4Address(host)
    bootstrap = Path(args.bootstrap).resolve(strict=True)
    replacement = Path(args.replacement).resolve(strict=True)
    assert cli.sha256(bootstrap) != cli.sha256(replacement), "Use two genuinely different builds"
    directory = Path(args.output).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    password = getpass.getpass("Windows SSH password (memory only): ")
    remote = remote_module.Remote(args.host, password)
    route = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    route.connect((host, 9))
    local_ip = route.getsockname()[0]
    route.close()
    suffix = time.strftime("%Y%m%d-%H%M%S")
    win_root = args.resume_bootstrap or f"C:/Users/{user}/Classroom-dev-update-{suffix}"
    if not re.fullmatch(r"C:/Users/" + re.escape(user) + r"/Classroom-dev-update-[0-9-]+", win_root):
        raise ValueError("Invalid isolated bootstrap path")
    task = "Classroom-Dev-Update-" + win_root.split("Classroom-dev-update-", 1)[1]
    program = win_root + "/ClassroomStudent.exe"
    inventory = remote.ps("$ErrorActionPreference='Stop'; Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,OSArchitecture | ConvertTo-Json; quser")
    print("WINDOWS PREFLIGHT", inventory.strip(), flush=True)
    print("BOOTSTRAP PATH", program, "TEACHER", local_ip, flush=True)
    if not args.resume_bootstrap:
        remote.ps(f"$ErrorActionPreference='Stop'; New-Item -ItemType Directory '{win_root}' | Out-Null")
    config = directory / "config.toml"
    config.write_text("[monitoring]\nenabled = false\n")
    worker = directory / "worker.ps1"
    # The task controller survives the original Student PID. It never watches
    # only the old child and kills the replacement immediately after self-update.
    worker.write_text(f"""$ErrorActionPreference='Stop'
$root='{win_root}'
$program=Join-Path $root 'ClassroomStudent.exe'
$task='{task}'
try {{
  $arguments=@('--teacher-ip','{local_ip}','--remember','--control-port','{args.port}',
    '--media-port','{args.port+10}','--feedback-port','{args.port+11}','--headless',
    '--config',(Join-Path $root 'config.toml'),'--launch-profile',(Join-Path $root 'profile.toml'),
    '--log-file',(Join-Path $root 'student.log'))
  $child=Start-Process $program -ArgumentList $arguments -WorkingDirectory $root -PassThru
  $child.Id | Set-Content (Join-Path $root 'bootstrap-pid.txt')
  Start-Sleep -Seconds 300
}} finally {{
  Get-CimInstance Win32_Process | Where-Object {{$_.ExecutablePath -and
    ($_.ExecutablePath.Replace('\\','/') -eq '{program}' -or
     $_.ExecutablePath.Replace('\\','/').StartsWith('{win_root}/.classroom-update-'))}} |
    ForEach-Object {{Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue}}
  Unregister-ScheduledTask -TaskName $task -Confirm:$false -ErrorAction SilentlyContinue
}}
""", encoding="utf-8-sig")
    if not args.resume_bootstrap:
        remote.copy(bootstrap, args.host + ":" + program)
        remote.copy(config, args.host + ":" + win_root + "/config.toml")
        remote.copy(worker, args.host + ":" + win_root + "/worker.ps1")
    else:
        print("RESUMING INITIAL BOOTSTRAP — no exe recopy", flush=True)
    verified = remote.ps(f"$ErrorActionPreference='Stop'; (Get-FileHash '{program}').Hash")
    assert cli.sha256(bootstrap).upper() in verified
    endpoint = directory / "teacher-endpoint.json"
    output = (directory / "teacher.stdout").open("wb")
    teacher = subprocess.Popen([args.teacher, "--role", "teacher", "--teacher-agent", "--bind-ip", local_ip,
        "--control-port", str(args.port), "--ipc-file", str(endpoint), "--config", str(config),
        "--launch-profile", str(directory / "teacher-profile.toml"), "--log-file", str(directory / "teacher.log")],
        stdout=output, stderr=output, start_new_session=True)
    client = None
    try:
        deadline = time.monotonic() + 20
        while not endpoint.exists():
            if teacher.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError("Isolated Teacher did not start; see teacher.log")
            time.sleep(.1)
        remote.ps(f"$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; Stop-ScheduledTask -TaskName '{task}' -ErrorAction SilentlyContinue; Get-CimInstance Win32_Process | Where-Object {{$_.ExecutablePath -and $_.ExecutablePath.Replace('\\','/') -eq '{program}'}} | ForEach-Object {{Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue}}; $code=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes((Get-Content -Raw '{win_root}/worker.ps1'))); $action=New-ScheduledTaskAction -Execute powershell.exe -Argument ('-NoProfile -EncodedCommand '+$code) -WorkingDirectory '{win_root}'; $principal=New-ScheduledTaskPrincipal -UserId '{user}' -LogonType Interactive -RunLevel Limited; $settings=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries; Register-ScheduledTask -TaskName '{task}' -Action $action -Principal $principal -Settings $settings -Force | Out-Null; Start-ScheduledTask '{task}'; $limit=(Get-Date).AddSeconds(20); while(-not(Test-Path '{win_root}/bootstrap-pid.txt') -and (Get-Date) -lt $limit){{Start-Sleep -Milliseconds 200}}; if(Test-Path '{win_root}/bootstrap-pid.txt'){{Get-Content '{win_root}/bootstrap-pid.txt'}}else{{Get-ScheduledTaskInfo '{task}' | ConvertTo-Json; throw 'Bootstrap Student did not launch'}}")
        print("BOOTSTRAP COMPLETE — no further SSH or SCP", flush=True)
        # All following transfer, replacement launch, reconnect and log collection
        # use Classroom connections, never remote.ps/copy/ssh.
        client = cli.IPC(endpoint)
        deadline = time.monotonic() + 30
        while True:
            ready = client.state.get("tools", {}).get("ready", [])
            rows = [s for s in client.state["students"] if s["ip"] == host and s["id"] in ready]
            if rows:
                break
            if time.monotonic() > deadline:
                raise TimeoutError("Windows Student did not connect; may need OS network approval")
            client.read()
        student_id = rows[0]["id"]
        before = client.state["tools"]["capabilities"][student_id]
        updates = []
        previous = before
        for index, build in enumerate([replacement] + ([Path(args.replacement2).resolve(strict=True)] if args.replacement2 else []), 1):
            package = directory / f"student-windows-new-{index}.zip"
            subprocess.run([sys.executable, str(ROOT / "scripts/classroom-dev-update.py"), "package", str(build),
                "--version", f"windows-dev-update-next-{index}", "--output", str(package)], check=True)
            with (directory / f"deploy-{index}.stdout").open("wb") as deploy_output:
                subprocess.run([sys.executable, str(ROOT / "scripts/classroom-dev-update.py"), "deploy", "--endpoint", str(endpoint),
                    "--package", str(package), "--student", student_id, "--timeout", "120"],
                    stdout=deploy_output, stderr=deploy_output, check=True, timeout=140)
            deadline = time.monotonic() + 10
            while True:
                client.read()
                after = client.state["tools"]["capabilities"].get(student_id, {})
                if after and after.get("build") != previous.get("build"):
                    break
                if time.monotonic() > deadline:
                    raise RuntimeError("Replacement build identity was not visible")
            assert previous.get("pid") != after.get("pid"), "Old Student PID must exit"
            updates.append({"before": previous, "after": after, "sha256": cli.sha256(build)})
            previous = after
            print("PASS: Windows update", index, "reconnected; build", after["build"], "PID", after.get("pid"), flush=True)
        client.command("collect_diagnostics", [student_id])
        deadline = time.monotonic() + 40
        while True:
            client.read()
            diagnostics = client.state.get("diagnostics", {})
            if diagnostics.get("phase") == "complete":
                assert diagnostics.get("collected") == 1, diagnostics
                break
            if diagnostics.get("phase") == "error" or time.monotonic() > deadline:
                raise RuntimeError(f"Diagnostic collection failed: {diagnostics}")
        (directory / "REPORT.json").write_text(json.dumps({"bootstrap_path":program,"teacher":local_ip,"student":host,
            "student_id":student_id,"before":before,"after":after,"bootstrap_sha256":cli.sha256(bootstrap),
            "replacement_sha256":cli.sha256(replacement),"updates":updates,"diagnostics":diagnostics,"post_bootstrap_ssh_calls":0}, indent=2))
        print("PASS: Windows logs collected through Teacher", diagnostics.get("path"), flush=True)
        print("Temporary Windows test worker stops its owned processes/task after 300 seconds; files/logs retained.", flush=True)
        client.command("quit")
    finally:
        if client:
            client.socket.close()
        if teacher.poll() is None:
            os.killpg(teacher.pid, signal.SIGTERM)
        teacher.wait(timeout=10)
        output.close()


if __name__ == "__main__":
    main()
