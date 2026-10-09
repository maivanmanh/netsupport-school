#!/usr/bin/env python3
"""Deploy the existing Mac/Windows Students into their interactive sessions.

Passwords come from CLASSROOM_MAC_SSH_PASSWORD / CLASSROOM_WINDOWS_SSH_PASSWORD
when set (for unattended runs), otherwise they are prompted; either way they
remain in memory only. This is a development launcher,
not an OS startup installer. Logs are copied locally without removing them.
"""
import argparse
import getpass
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shlex
import subprocess
import time

from importlib.util import spec_from_file_location, module_from_spec

ROOT = Path(__file__).resolve().parents[1]
spec = spec_from_file_location('remote', ROOT / 'scripts/run-four-machine-test.py')
remote = module_from_spec(spec)
spec.loader.exec_module(remote)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--teacher', default='192.168.50.206')
    parser.add_argument('--collect-only', action='store_true')
    parser.add_argument('--mac-only', action='store_true')
    parser.add_argument('--mac-host', default='mvmanh@macstudio')
    parser.add_argument('--windows-host', default='mvmanh@192.168.50.94')
    parser.add_argument('--development-ad-hoc', action='store_true',
                        help='Explicit local development bundle; macOS permissions may require reapproval')
    args = parser.parse_args()
    mac_app = ROOT / 'target/Classroom Student.app'
    if not args.collect_only:
        # Fail before touching remote processes or shipping a blocked bundle.
        if args.development_ad_hoc:
            subprocess.run(['codesign', '--verify', '--deep', '--strict', str(mac_app)], check=True)
            detail = subprocess.check_output(['codesign', '--display', '--verbose=2', str(mac_app)],
                                             stderr=subprocess.STDOUT, text=True)
            assert 'Signature=adhoc' in detail, 'Expected explicit development signature'
        else:
            subprocess.run(['python3', str(ROOT / 'scripts/sign-macos-app.py'),
                            str(mac_app), '--verify-only'], check=True)
    def password(variable, prompt):
        return os.environ.get(variable) or getpass.getpass(prompt)
    mac = remote.Remote(args.mac_host, password('CLASSROOM_MAC_SSH_PASSWORD', 'Mac SSH password: '))
    win = None if args.mac_only else remote.Remote(
        args.windows_host, password('CLASSROOM_WINDOWS_SSH_PASSWORD', 'Windows SSH password: '))
    folder = ROOT / 'target/control-responsiveness'
    folder.mkdir(parents=True, exist_ok=True)
    mac_root = '/Users/mvmanh/Classroom-two-mac'
    win_root = 'C:/Users/mvmanh/Classroom-two-mac'
    print('MAC PREFLIGHT ' + mac.ssh('id -u; uname -m; ipconfig getifaddr en0 || true').strip(), flush=True)
    if win:
        print('WINDOWS PREFLIGHT ' + win.ps('[Environment]::UserName; quser').strip(), flush=True)
    if not args.collect_only:
        mac.ssh(f'mkdir -p {mac_root}/bin {mac_root}/logs; launchctl bootout gui/$(id -u)/com.classroom.dev.student 2>/dev/null || true')
        # LaunchServices can own a manually opened copy outside this job.
        # Stop only this exact installed Student before replacing the bundle.
        mac.ssh(f'''for signal in TERM KILL; do
  ps -ww -axo pid=,comm= | while read -r pid executable; do
    case "$executable" in
      {mac_root}/*/Contents/MacOS/ClassroomStudent|{mac_root}/bin/Classroom)
        kill -"$signal" "$pid" 2>/dev/null || true ;;
    esac
  done
  if [ "$signal" = TERM ]; then sleep 2; fi
done''')
        if win:
            win.ps(f"Stop-ScheduledTask -TaskName Classroom-Dev-Student -ErrorAction SilentlyContinue; Get-CimInstance Win32_Process | Where-Object {{$_.ExecutablePath -and $_.ExecutablePath.Replace('\\','/') -eq '{win_root}/bin/Classroom.exe'}} | ForEach-Object {{Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue}}; New-Item -ItemType Directory -Force '{win_root}/bin','{win_root}/logs' | Out-Null")
        mac_file = mac_app / 'Contents/MacOS/ClassroomStudent'
        subprocess.run(['codesign', '--verify', '--deep', '--strict', str(mac_app)], check=True)
        archive = folder / 'student-app.zip'
        subprocess.run(['ditto', '-c', '-k', '--keepParent', '--sequesterRsrc', str(mac_app), str(archive)], check=True)
        win_file = ROOT / 'target/rustup-windows/x86_64-pc-windows-gnu/release/ClassroomStudent.exe'
        mac.copy(archive, mac.address + ':' + mac_root + '/student-app.zip')
        installed_app = mac_root + '/Classroom Student.app'
        installed_binary = installed_app + '/Contents/MacOS/ClassroomStudent'
        app = shlex.quote(installed_app)
        assessment = (f'codesign --display --verbose=2 {app} 2>&1' if args.development_ad_hoc
                      else f'spctl --assess --type execute --verbose=4 {app}')
        mac_result = mac.ssh(f'set -e; if [ -d {app} ]; then mv {app} {shlex.quote(installed_app + ".previous-" + str(int(time.time())))}; fi; ditto -x -k {mac_root}/student-app.zip {mac_root}; codesign --verify --deep --strict {app}; {assessment}; ln -sfn {shlex.quote("../Classroom Student.app/Contents/MacOS/ClassroomStudent")} {mac_root}/bin/Classroom')
        if args.development_ad_hoc:
            assert 'Signature=adhoc' in mac_result, 'Remote signature differs'
        if win:
            win.copy(win_file, win.address + ':' + win_root + '/bin/Classroom.exe')
            # Remove an unrelated stale installed Student/Guard only after the
            # replacement has been copied and its checksum can be verified.
            expected = hashlib.sha256(win_file.read_bytes()).hexdigest()
            migration = win.remove_old_student(win_root + '/bin/Classroom.exe', expected)
            (folder / 'windows-old-install-removal.txt').write_text(migration)

        mac_sha = hashlib.sha256(mac_file.read_bytes()).hexdigest()
        win_sha = hashlib.sha256(win_file.read_bytes()).hexdigest() if win else None
        assert mac_sha in mac.ssh(f'shasum -a 256 {shlex.quote(installed_binary)}')
        local_req = subprocess.check_output(['codesign', '--display', '-r-', str(mac_app)], stderr=subprocess.STDOUT, text=True).split('designated => ', 1)[1].strip()
        remote_req = mac.ssh(f'codesign --display -r- {app} 2>&1')
        assert local_req in remote_req, 'Remote signing identity differs'
        if win:
            assert win_sha.upper() in win.ps(f"(Get-FileHash '{win_root}/bin/Classroom.exe').Hash")
        plist = folder / 'student.plist'
        plist.write_bytes(plistlib.dumps({
            'Label': 'com.classroom.dev.student',
            'ProgramArguments': [installed_binary, '--role', 'student', '--teacher-ip', args.teacher, '--remember', '--log-file', mac_root + '/logs/student.log.txt'],
            'WorkingDirectory': mac_root,
            'StandardOutPath': mac_root + '/logs/student.stdout.txt',
            'StandardErrorPath': mac_root + '/logs/student.stderr.txt',
            'RunAtLoad': True,
            # Standard launchd jobs coalesce timers and throttle resources.
            # A live classroom agent needs application-like scheduling even
            # when its LSUIElement window is hidden.
            'ProcessType': 'Interactive',
            'LegacyTimers': True,
            'KeepAlive': {'SuccessfulExit': False},
        }))
        mac.copy(plist, mac.address + ':' + mac_root + '/student.plist')
        mac.ssh(f'launchctl bootstrap gui/$(id -u) {mac_root}/student.plist')
        program = win_root + '/bin/Classroom.exe'
        if win:
            win.ps(f"$ErrorActionPreference='Stop'; $a=New-ScheduledTaskAction -Execute '{program}' -Argument '--role student --teacher-ip {args.teacher} --remember --log-file {win_root}/logs/student.log.txt' -WorkingDirectory '{win_root}'; $p=New-ScheduledTaskPrincipal -UserId mvmanh -LogonType Interactive -RunLevel Limited; $s=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries; Register-ScheduledTask -TaskName Classroom-Dev-Student -Action $a -Principal $p -Settings $s -Force | Out-Null; Start-ScheduledTask Classroom-Dev-Student")
        (folder / 'deployment.json').write_text(json.dumps({'teacher': args.teacher, 'mac_sha256': mac_sha, 'windows_sha256': win_sha, 'mac_binary': installed_binary, 'mac_requirement': local_req, 'windows_binary': program if win else None, 'created_unix': time.time()}, indent=2))
        print('DEPLOYED AND STARTED STUDENTS', flush=True)
        # Owner requests replacement, not an accumulating set of old apps.
        # Prune only deployment-owned bundle backups after verified installation.
        mac.ssh("python3 - <<'PY'\nfrom pathlib import Path\nimport shutil\nroot = Path.home() / 'Classroom-two-mac'\nfor old in root.glob('Classroom Student.app.previous-*'):\n    if old.is_dir() and not old.is_symlink():\n        shutil.rmtree(old)\nPY")
    hosts = [(mac, mac_root, 'mac')]
    if win:
        hosts.append((win, win_root, 'windows'))
    for host, root, name in hosts:
        host.copy(host.address + ':' + root + '/logs', folder / name, recursive=True)
        print('COLLECTED ' + name, flush=True)


if __name__ == '__main__':
    main()
