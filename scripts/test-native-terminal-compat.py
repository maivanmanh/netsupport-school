#!/usr/bin/env python3
"""Owned temporary Windows CMD compatibility integration workload via authenticated IPC."""
import importlib.util,json,socket,struct,time,getpass,sys
from pathlib import Path
s=importlib.util.spec_from_file_location('ipc','scripts/classroom-dev-update.py');m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
endpoint=Path(sys.argv[sys.argv.index('--endpoint')+1] if '--endpoint' in sys.argv else 'target/source-switch-endpoint.json');c=m.IPC(endpoint)
while not c.state.get('students'):c.read()
student=next(r['id'] for r in c.state['students'] if r['ip']=='192.168.50.94' and r['online'])
r=None
if '--network' in sys.argv:
    rs=importlib.util.spec_from_file_location('remote','scripts/run-four-machine-test.py');rm=importlib.util.module_from_spec(rs);rs.loader.exec_module(rm)
    r=rm.Remote('mvmanh@192.168.50.94',getpass.getpass('SSH password: '))
    c=m.IPC(endpoint)
session=c.command('terminal_open',[student])['result']['session']
d=json.loads(endpoint.read_text());sock=None;last=0;out=bytearray();ready=None
def attach():
    global sock
    sock=socket.create_connection(('127.0.0.1',d['port']),timeout=8)
    b=json.dumps(dict(type='hello',protocol=1,token=d['token'],ui_role='terminal',session=session,last_seq=last)).encode();sock.sendall(struct.pack('!I',len(b))+b)
def send(k,b=b'',seq=0):sock.sendall(struct.pack('!BQI',k,seq,len(b))+b)
def exact(n):
    b=bytearray()
    while len(b)<n:
        x=sock.recv(n-len(b))
        if not x:raise EOFError('terminal closed')
        b.extend(x)
    return b
def read_until(marker,seconds=15):
    global last,ready
    if marker and marker in out:return
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline:
        k,seq,n=struct.unpack('!BQI',exact(13));b=exact(n)
        if k==1:
            if seq>last:
                assert seq==last+1,(last,seq)
                out.extend(b);last=seq
                if b'\x1b[6n' in b:send(2,b'\x1b[1;1R')
            send(7,seq=seq)
            if marker and marker in out:return
        elif k==5:
            ready=json.loads(b)
            send(8)
            if not marker and ready.get('state')=='ready':return
        elif k==6:raise RuntimeError('shell exited: '+b.decode(errors='replace'))
    raise TimeoutError('terminal response timeout')

import uuid,base64,re
fixture='ClassroomTerminalTest_'+uuid.uuid4().hex
case=0

def run(command,expected=None,code=0,seconds=30):
    global case
    case+=1; start=len(out)
    send(2,command.encode('utf-8')+b'\r')
    # Separate input lines preserve CMD's per-line ERRORLEVEL expansion.
    marker=f'CHECK_{case}_'
    send(2,f'echo {marker}%ERRORLEVEL%_DONE\r'.encode())
    read_until(f'{marker}{code}_DONE'.encode(),seconds)
    result=bytes(out[start:])
    if expected is not None:assert expected.encode('utf-8') in result,(command,result[-400:])
    print(f'case_{case}: pass ({command.split()[0]})')
    return result

try:
    attach();read_until(None);assert ready['os']=='windows' and not ready['elevated'];read_until(b'>')
    run('chcp 65001 >nul')
    run(f'mkdir "%TEMP%\\{fixture}"')
    run(f'cd /d "%TEMP%\\{fixture}"')
    run('touch "đề bài.txt"')
    run('echo Alpha beta>"đề bài.txt"')
    run('cat "đề bài.txt"','Alpha beta')
    run('cp "đề bài.txt" "copy.txt"')
    run('mv "copy.txt" "moved.txt"')
    run('ls -la','moved.txt')
    run('ls -lh','moved.txt')
    run('pwd',fixture)
    run('head -n 1 "đề bài.txt"','Alpha beta')
    run('tail -n 1 "đề bài.txt"','Alpha beta')
    run('grep -in alpha "đề bài.txt"','1:Alpha beta')
    run('cat "đề bài.txt" | grep -in alpha > "pipe result.txt"')
    run('cat "pipe result.txt"','1:Alpha beta')
    run('printenv OS','Windows_NT')
    run('export RT_COMPAT="hello world"')
    run('echo %RT_COMPAT%','hello world')
    run('export RT_UNICODE="đề bài"')
    run('printenv RT_UNICODE','đề bài')
    run('env','OS=Windows_NT')
    run('which cmd','cmd.exe')
    run('uname -s','Windows')
    run('uname -r','Version')
    run('ps aux','cmd.exe')
    run('wc -l "đề bài.txt"','1 ')
    run('du -h "đề bài.txt"','KiB')
    run('df','DeviceID',seconds=40)
    run('basename "folder space\\file.txt"','file.txt')
    run('dirname "folder space\\file.txt"','folder space')
    run('realpath "moved.txt"',fixture)
    run('history')
    run('man dir',seconds=40)
    run('sleep 0.01')
    run('mkdir "empty dir"');run('rmdir "empty dir"')
    run('date /t');run('whoami','mvmanh')
    run('clear')
    run('rm --bad "moved.txt"',code=2)
    run('cat "moved.txt"','Alpha beta')
    run('rm "moved.txt"')
    run('mkdir "source dir"');run('touch "source dir\\one.txt"')
    run('cp -r "source dir" "destination dir"')
    run('rm -rf "destination dir"')
    run('cat "source dir\\one.txt"')
    run('kill NOT_A_PID',code=2)
    run('open "đề bài.txt"',code=2)
    # A current-directory command and a DOSKEY alias must win over helpers.
    run('echo @echo CUSTOM_NATIVE_PRIORITY>ls.cmd')
    run('ls','CUSTOM_NATIVE_PRIORITY')
    run('doskey ls=echo ALIAS_NATIVE_PRIORITY')
    run('ls','ALIAS_NATIVE_PRIORITY')
    run('doskey ls=');run('del ls.cmd')
    run('echo first>follow.txt')
    ps="Start-Sleep -Seconds 2; Add-Content -LiteralPath 'follow.txt' -Value 'FOLLOW_APPENDED'"
    encoded=base64.b64encode(ps.encode('utf-16le')).decode()
    run('start /b powershell -NoProfile -EncodedCommand '+encoded)
    send(2,b'tail -f follow.txt\r');read_until(b'FOLLOW_APPENDED',15)
    send(2,b'\x03');time.sleep(.5);run('echo INTERRUPTED_%OS%','INTERRUPTED_Windows_NT')
    run('cd /d "%USERPROFILE%"');run(f'rmdir /s /q "%TEMP%\\{fixture}"')
    print('compat_integration_pass: true')
finally:
    if sock:
        try:send(4)
        except OSError:pass
        sock.close()
    c=m.IPC(endpoint);c.command('terminal_close',data={'session':session})
