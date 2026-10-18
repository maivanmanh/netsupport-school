//! Optional LAN ZIP download, separate from classroom control/media and private IPC.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{watch, Semaphore},
    task::JoinHandle,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bootstrap {
    pub teacher_ip: Ipv4Addr,
    pub control_port: u16,
    pub connection_popup: bool,
}
impl Bootstrap {
    pub fn load(directory: &Path) -> Result<Option<Self>> {
        let path = directory.join("classroom-bootstrap.toml");
        if !path.exists() {
            return Ok(None);
        }
        anyhow::ensure!(
            fs::metadata(&path)?.len() <= 4096,
            "Bootstrap config too large"
        );
        let value: Self = toml::from_str(&fs::read_to_string(path)?)?;
        anyhow::ensure!(
            !value.teacher_ip.is_unspecified()
                && !value.teacher_ip.is_multicast()
                && value.teacher_ip != Ipv4Addr::BROADCAST,
            "Invalid bootstrap Teacher IP"
        );
        anyhow::ensure!(
            (1024..=65524).contains(&value.control_port),
            "Invalid bootstrap control port"
        );
        Ok(Some(value))
    }
}
pub fn bundled_package() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .skip(1)
        .take(7)
        .flat_map(|parent| {
            [
                parent.join("student-packages/student-windows-x64.zip"),
                parent.join("Resources/student-packages/student-windows-x64.zip"),
            ]
        })
        .find(|p| p.is_file())
}

pub struct Share {
    /// Where the installer script copy lives; removed with the share.
    directory: PathBuf,
    stop: watch::Sender<bool>,
    worker: JoinHandle<()>,
    cancelled: Arc<AtomicBool>,
    pub state: Arc<Mutex<serde_json::Value>>,
}
impl Share {
    pub fn start(
        source: PathBuf,
        directory: PathBuf,
        ip: Ipv4Addr,
        port: u16,
        control_port: u16,
        popup: bool,
    ) -> Result<Self> {
        anyhow::ensure!(
            port != 0 && ![control_port, control_port + 1, control_port + 2].contains(&port),
            "Download port conflicts with classroom listeners"
        );
        // All interfaces, not just the one the share started on: if the Teacher moves
        // from the LAN cable to Wi-Fi the link keeps working, and each download
        // is built for the address the Student actually reached.
        let listener = std::net::TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).context(
            "Cannot open Student download port; choose a free port or allow it in the firewall",
        )?;
        listener.set_nonblocking(true)?;
        let listener = TcpListener::from_std(listener)?;
        let state = Arc::new(Mutex::new(
            serde_json::json!({"phase":"preparing","url":url(ip,port)}),
        ));
        let status = Arc::clone(&state);
        let script_directory = directory.clone();
        let (stop, mut stopped) = watch::channel(false);
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel_prepare = Arc::clone(&cancelled);
        let cleanup_directory = directory.clone();
        let worker = tokio::spawn(async move {
            let refresher = Arc::new(Refresher {
                source,
                directory,
                port,
                control_port,
                popup,
                cancel: cancel_prepare,
            });
            let first = Arc::clone(&refresher);
            let preparation = tokio::task::spawn_blocking(move || first.build(ip));
            // Preparation is bounded; its result owns cleanup even when this task is dropped.
            let package = match preparation.await {
                Ok(Ok(package)) => package,
                result => {
                    *status.lock().unwrap() =
                        serde_json::json!({"phase":"error","error":format!("{result:?}")});
                    return;
                }
            };
            *status.lock().unwrap() = serde_json::json!({"phase":"sharing","url":url(ip,port),"script_url":script_url(ip,port),"script_path":package.script_path,"size":package.size,"downloads":0,"arch":package.arch});
            tracing::info!(%ip,port,bytes=package.size,"Student ZIP sharing ready");
            let current = Arc::new(tokio::sync::Mutex::new(Arc::clone(&package)));
            let slots = Arc::new(Semaphore::new(4));
            let mut tasks = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = stopped.changed() => break,
                    accepted = listener.accept() => {
                        let Ok((socket,peer)) = accepted else { break };
                        if tasks.len() < 48 {
                            let permit = Arc::clone(&slots).try_acquire_owned();
                            let current = Arc::clone(&current);
                            let refresher = Arc::clone(&refresher);
                            let status = Arc::clone(&status);
                            tasks.spawn(async move {
                                let Ok(_permit) = permit else {
                                    let mut socket=socket;
                                    let body=b"<html><head><meta http-equiv=refresh content=3></head><body>Classroom: waiting for a download slot. Retrying...</body></html>";
                                    let response=format!("HTTP/1.1 503 Service Unavailable\r\nRetry-After: 3\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
                                    let _=tokio::time::timeout(std::time::Duration::from_secs(1),async {socket.write_all(response.as_bytes()).await?;socket.write_all(body).await}).await;
                                    return;
                                };
                                // The package for the address this Student reached us on.
                                let package = refresher
                                    .package_for(&current, socket.local_addr().ok().map(|a| a.ip()), &status)
                                    .await;
                                let result = serve(socket,package).await;
                                if matches!(result,Ok(true)) {tracing::info!(%peer,"Student ZIP downloaded");}
                                if matches!(result,Ok(true)) { let mut value=status.lock().unwrap(); let n=value["downloads"].as_u64().unwrap_or(0);value["downloads"]=(n+1).into(); }
                                if let Err(error)=result {tracing::debug!(%peer,%error,"Student ZIP download ended");}
                            });
                        } // Excess connections are closed; no unbounded task/accept queue.
                    }
                    _ = tasks.join_next(), if !tasks.is_empty() => {}
                }
            }
            tracing::info!(%ip,port,"Student ZIP sharing stopped");
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
            let _ = fs::remove_file(cleanup_directory.join(SCRIPT_NAME));
        });
        Ok(Self {
            stop,
            worker,
            cancelled,
            state,
            directory: script_directory,
        })
    }
}
impl Drop for Share {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.stop.send(true);
        self.worker.abort(); // release listener/download tasks immediately, including preparation
        let _ = fs::remove_file(self.directory.join(SCRIPT_NAME));
        tracing::info!("Student ZIP sharing disabled");
    }
}
fn url(ip: Ipv4Addr, port: u16) -> String {
    if port == 80 {
        format!("http://{ip}/")
    } else {
        format!("http://{ip}:{port}/")
    }
}
fn script_url(ip: Ipv4Addr, port: u16) -> String {
    format!("{}{SCRIPT_PATH}", url(ip, port).trim_end_matches('/'))
}
/// Where the installer script is served (the ZIP stays at `/`).
const SCRIPT_PATH: &str = "/install.bat";

#[derive(Debug)]
struct Package {
    path: PathBuf,
    size: u64,
    arch: String,
    /// What the installer script needs. The script itself is made for each
    /// request (see [`Package::script_for`]) so it always names the Teacher's
    /// address as the Student reached it; a copy is also written next to the
    /// ZIP for the Teacher to hand out.
    sha256: String,
    port: u16,
    control_port: u16,
    ip: Ipv4Addr,
    script_path: PathBuf,
}
impl Package {
    /// The installer for a Student that connected to us on `local` (falls back to
    /// the address the share was started on).
    fn script_for(&self, local: Option<std::net::IpAddr>) -> String {
        let ip = match local {
            Some(std::net::IpAddr::V4(v4)) if !v4.is_loopback() && !v4.is_unspecified() => v4,
            _ => self.ip,
        };
        install_script(ip, self.port, self.control_port, &self.sha256)
    }
}
impl Drop for Package {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        // The script copy is shared by every rebuild and removed with the share.
    }
}

/// Builds the package for a Teacher address, again whenever Students start
/// reaching the Teacher on a different one (cable to Wi-Fi, another room).
struct Refresher {
    source: PathBuf,
    directory: PathBuf,
    port: u16,
    control_port: u16,
    popup: bool,
    cancel: Arc<AtomicBool>,
}
impl Refresher {
    fn build(&self, ip: Ipv4Addr) -> Result<Arc<Package>> {
        prepare(
            &self.source,
            &self.directory,
            &Bootstrap {
                teacher_ip: ip,
                control_port: self.control_port,
                connection_popup: self.popup,
            },
            self.port,
            &self.cancel,
        )
    }

    /// The address worth rebuilding for, if `local` (where a Student connected)
    /// differs from the one `package` was built for.
    fn address_to_rebuild_for(package: &Package, local: Option<std::net::IpAddr>) -> Option<Ipv4Addr> {
        match local {
            Some(std::net::IpAddr::V4(v4))
                if !v4.is_loopback() && !v4.is_unspecified() && v4 != package.ip =>
            {
                Some(v4)
            }
            _ => None,
        }
    }

    async fn package_for(
        self: &Arc<Self>,
        current: &tokio::sync::Mutex<Arc<Package>>,
        local: Option<std::net::IpAddr>,
        status: &Mutex<serde_json::Value>,
    ) -> Arc<Package> {
        let mut held = current.lock().await;
        if let Some(ip) = Self::address_to_rebuild_for(&held, local) {
            let this = Arc::clone(self);
            if let Ok(Ok(package)) = tokio::task::spawn_blocking(move || this.build(ip)).await {
                tracing::info!(%ip, "Student share rebuilt for the Teacher's new address");
                let mut value = status.lock().unwrap();
                value["url"] = url(ip, self.port).into();
                value["script_url"] = script_url(ip, self.port).into();
                value["script_path"] = serde_json::json!(package.script_path);
                *held = package;
            }
        }
        Arc::clone(&held)
    }
}

/// Name of the installer script, both as downloaded and as saved for the Teacher.
pub const SCRIPT_NAME: &str = "Install-Classroom-Student.bat";

/// A Windows batch file that installs the Student from this Teacher with no
/// other step: run it as administrator (or just open it and accept the prompt)
/// and it downloads the ZIP, checks it, unpacks it into `Program Files\Classroom`
/// and installs the Guard service, which starts the Student in the signed-in
/// user's session. Pass `/quiet` to skip the final pause (remote execution).
///
/// ASCII only and CRLF, because cmd.exe reads it in the console code page.
pub fn install_script(ip: Ipv4Addr, port: u16, control_port: u16, sha256: &str) -> String {
    // ":8080", or nothing for the default web port.
    let suffix = if port == 80 { String::new() } else { format!(":{port}") };
    let base = format!("http://{ip}{suffix}");
    let template = r#"@echo off
setlocal EnableExtensions
rem  Kept now: SHIFT below changes what %~f0 means.
set "SELF=%~f0"
title Cai dat Classroom Student
set "CLASSROOM_BASE=@BASE@"
set "CLASSROOM_TEACHER_IP=@IP@"
set "CLASSROOM_CONTROL_PORT=@CONTROL@"
set "CLASSROOM_SHA256=@SHA@"
set "CLASSROOM_DIR=%ProgramFiles%\Classroom"
set "QUIET=0"
set "SCRIPT_ARGS=%*"
:args
if "%~1"=="" goto args_done
if /i "%~1"=="/quiet" set "QUIET=1"
if /i "%~1"=="-quiet" set "QUIET=1"
rem  /teacher <IP>: use another Teacher address than the one this file was made with.
if /i "%~1"=="/teacher" (
  set "CLASSROOM_TEACHER_IP=%~2"
  set "CLASSROOM_BASE=http://%~2@PORTSUFFIX@"
  shift
)
shift
goto args
:args_done

fltmc >nul 2>&1
if errorlevel 1 (
  if "%QUIET%"=="1" (
    echo [LOI] Can chay bang quyen Administrator.
    exit /b 5
  )
  echo Dang xin quyen Administrator...
  if defined SCRIPT_ARGS (
    powershell -NoProfile -Command "Start-Process -FilePath '%SELF%' -ArgumentList '%SCRIPT_ARGS%' -Verb RunAs"
  ) else (
    powershell -NoProfile -Command "Start-Process -FilePath '%SELF%' -Verb RunAs"
  )
  exit /b
)

echo ==============================================
echo  Cai dat Classroom Student
echo  May giang vien: %CLASSROOM_BASE%
echo ==============================================
powershell -NoProfile -Command "$t=[IO.File]::ReadAllText('%SELF%'); Invoke-Expression $t.Substring($t.LastIndexOf('#PS-BEGIN'))"
set "RESULT=%errorlevel%"
if not "%QUIET%"=="1" pause
exit /b %RESULT%
#PS-BEGIN
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 } catch {}
$base = $env:CLASSROOM_BASE
$dir = $env:CLASSROOM_DIR
$work = Join-Path $env:TEMP ('classroom-install-' + [guid]::NewGuid().ToString('N'))
function Step($text) { Write-Host ('[+] ' + $text) }
$code = 0
try {
    New-Item -ItemType Directory -Path $work | Out-Null
    $zip = Join-Path $work 'student.zip'
    Step 'Dang tai goi cai dat tu may giang vien...'
    $done = $false
    for ($attempt = 1; $attempt -le 60 -and -not $done; $attempt++) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri ($base + '/') -OutFile $zip -TimeoutSec 900
            $done = $true
        } catch {
            if ($attempt -ge 60) { throw ('Khong tai duoc tu ' + $base + ' - ' + $_.Exception.Message) }
            Start-Sleep -Seconds 3
        }
    }
    Step 'Dang kiem tra goi cai dat...'
    $hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLower()
    if ($hash -ne $env:CLASSROOM_SHA256) { throw 'Goi tai ve bi loi (sai ma kiem tra). Hay chay lai.' }
    Step 'Dang dung ban Student dang chay (neu co)...'
    $service = Get-Service -Name ClassroomGuardService -ErrorAction SilentlyContinue
    if ($service -and $service.Status -ne 'Stopped') {
        Stop-Service -Name ClassroomGuardService -Force
        $service.WaitForStatus('Stopped', '00:00:30')
    }
    Get-Process -Name ClassroomStudent -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 1
    Step ('Dang giai nen vao ' + $dir)
    $stage = Join-Path $work 'files'
    Expand-Archive -LiteralPath $zip -DestinationPath $stage -Force
    if (-not (Test-Path -LiteralPath (Join-Path $stage 'ClassroomStudent.exe'))) { throw 'Goi cai dat khong co ClassroomStudent.exe' }
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    Copy-Item -Path (Join-Path $stage '*') -Destination $dir -Recurse -Force
    # The address and port of THIS Teacher, whatever the ZIP was built with.
    $toml = Join-Path $dir 'classroom-bootstrap.toml'
    if (Test-Path -LiteralPath $toml) {
        $fixed = Get-Content -LiteralPath $toml | ForEach-Object {
            if ($_ -match '^\s*teacher_ip\s*=') { 'teacher_ip = "' + $env:CLASSROOM_TEACHER_IP + '"' }
            elseif ($_ -match '^\s*control_port\s*=') { 'control_port = ' + $env:CLASSROOM_CONTROL_PORT }
            else { $_ }
        }
        Set-Content -LiteralPath $toml -Value $fixed -Encoding ASCII
    }
    Get-ChildItem -LiteralPath $dir -Recurse -File | Unblock-File -ErrorAction SilentlyContinue
    $guard = Join-Path $dir 'ClassroomGuardService.exe'
    $student = Join-Path $dir 'ClassroomStudent.exe'
    if (Test-Path -LiteralPath $guard) {
        Step 'Dang cai dich vu Classroom Guard (tu mo lai Student khi bi tat)...'
        $install = Start-Process -FilePath $guard -ArgumentList @('--install', '--teacher-ip', $env:CLASSROOM_TEACHER_IP, '--control-port', $env:CLASSROOM_CONTROL_PORT) -WorkingDirectory $dir -Wait -PassThru -WindowStyle Hidden
        if ($install.ExitCode -ne 0) { throw ('Cai dich vu that bai (ma ' + $install.ExitCode + ')') }
        $service = Get-Service -Name ClassroomGuardService -ErrorAction SilentlyContinue
        if (-not $service -or $service.Status -ne 'Running') { throw 'Dich vu Classroom Guard chua chay sau khi cai.' }
    } else {
        Step 'Dang chay Student...'
        Start-Process -FilePath $student -WorkingDirectory $dir
    }
    Step 'Dang cho Student khoi dong...'
    $running = $false
    for ($i = 0; $i -lt 40 -and -not $running; $i++) {
        if (Get-Process -Name ClassroomStudent -ErrorAction SilentlyContinue) { $running = $true } else { Start-Sleep -Seconds 1 }
    }
    if (-not $running) { throw 'Student chua chay. Hay dang nhap Windows roi chay lai, hoac kiem tra tuong lua.' }
    Write-Host ''
    Write-Host 'HOAN TAT: Classroom Student da duoc cai va dang chay.' -ForegroundColor Green
} catch {
    Write-Host ''
    Write-Host ('LOI: ' + $_.Exception.Message) -ForegroundColor Red
    $code = 1
} finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
exit $code
"#;
    template
        .replace("@BASE@", &base)
        .replace("@IP@", &ip.to_string())
        .replace("@CONTROL@", &control_port.to_string())
        .replace("@SHA@", sha256)
        .replace("@PORTSUFFIX@", &suffix)
        .replace("\r\n", "\n")
        .replace("\n", "\r\n")
}
fn prepare(
    source: &Path,
    directory: &Path,
    settings: &Bootstrap,
    port: u16,
    cancel: &AtomicBool,
) -> Result<Arc<Package>> {
    anyhow::ensure!(
        !cancel.load(Ordering::Acquire),
        "Student ZIP preparation cancelled"
    );
    let manifest = super::dev_update::manifest(source)?;
    anyhow::ensure!(
        manifest.os == "windows"
            && manifest.root == manifest.executable
            && manifest
                .executable
                .eq_ignore_ascii_case("ClassroomStudent.exe"),
        "Choose a Windows Student dev-update ZIP containing a portable executable"
    );
    fs::create_dir_all(directory)?;
    let mut random = [0u8; 12];
    getrandom::fill(&mut random).map_err(|e| anyhow::anyhow!("Package nonce: {e}"))?;
    let path = directory.join(format!(
        "student-download-{:x}.zip",
        u128::from_le_bytes({
            let mut b = [0; 16];
            b[..12].copy_from_slice(&random);
            b
        })
    ));
    let result = (|| -> Result<Arc<Package>> {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut output = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let mut input = zip::ZipArchive::new(fs::File::open(source)?)?;
        {
            // Scoped so this entry's borrow of `input` ends before looking
            // up the Guard service entry below — the zip crate only allows
            // one open entry from an archive at a time.
            let mut entry = input.by_name(&manifest.executable)?;
            anyhow::ensure!(
                entry.size() <= super::dev_update::MAX_PACKAGE
                    && entry.unix_mode().is_none_or(|m| m & 0o170_000 != 0o120_000),
                "Invalid Student executable"
            );
            output.start_file("ClassroomStudent.exe", options)?;
            let mut digest = Sha256::new();
            let mut buffer = vec![0u8; 64 * 1024];
            let mut total = 0u64;
            loop {
                anyhow::ensure!(
                    !cancel.load(Ordering::Acquire),
                    "Student ZIP preparation cancelled"
                );
                let n = entry.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                total += n as u64;
                anyhow::ensure!(
                    total <= super::dev_update::MAX_PACKAGE,
                    "Student executable too large"
                );
                digest.update(&buffer[..n]);
                output.write_all(&buffer[..n])?;
            }
            anyhow::ensure!(
                format!("{:x}", digest.finalize()) == manifest.sha256,
                "Student executable checksum mismatch"
            );
        }
        // Carried through whenever the source dev-update package has one
        // (an older package built before the Guard service existed won't):
        // the Student self-installs it at startup when run elevated, so
        // shipping both together means a Teacher-shared download is a
        // complete, protectable install, not just the plain Student.
        let bundled_guard = match input.by_name("ClassroomGuardService.exe") {
            Ok(mut guard_entry) => {
                anyhow::ensure!(
                    guard_entry.size() <= super::dev_update::MAX_PACKAGE
                        && guard_entry.unix_mode().is_none_or(|m| m & 0o170_000 != 0o120_000),
                    "Invalid Guard service executable"
                );
                output.start_file("ClassroomGuardService.exe", options)?;
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    anyhow::ensure!(
                        !cancel.load(Ordering::Acquire),
                        "Student ZIP preparation cancelled"
                    );
                    let n = guard_entry.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    output.write_all(&buffer[..n])?;
                }
                true
            }
            Err(zip::result::ZipError::FileNotFound) => false,
            Err(error) => return Err(error.into()),
        };
        output.start_file("classroom-bootstrap.toml", options)?;
        output.write_all(toml::to_string(settings)?.as_bytes())?;
        output.start_file("README.txt", options)?;
        output.write_all(format!(
            "Classroom Student ({})\r\nGiai nen toan bo ZIP vao thu muc local, roi double click ClassroomStudent.exe.\r\nTeacher: {}:{}\r\nKhong chay executable truc tiep ben trong ZIP.\r\n{}",
            manifest.arch, settings.teacher_ip, settings.control_port,
            if bundled_guard {
                "\r\nDe bat bao ve chong tat (ClassroomGuardService), chuot phai ClassroomStudent.exe > Run as administrator mot lan; no se tu cai va chay dich vu bao ve.\r\n"
            } else {
                ""
            },
        ).as_bytes())?;
        let file = output.finish()?;
        file.sync_all()?;
        let size = file.metadata()?.len();
        let mut digest = Sha256::new();
        let mut reader = fs::File::open(&path)?;
        let mut block = vec![0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut block)?;
            if n == 0 {
                break;
            }
            digest.update(&block[..n]);
        }
        let sha256 = format!("{:x}", digest.finalize());
        let script_path = directory.join(SCRIPT_NAME);
        let package = Package {
            path: path.clone(),
            size,
            arch: manifest.arch,
            sha256,
            port,
            control_port: settings.control_port,
            ip: settings.teacher_ip,
            script_path,
        };
        fs::write(&package.script_path, package.script_for(None).as_bytes())?;
        Ok(Arc::new(package))
    })();
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}
async fn serve(mut socket: tokio::net::TcpStream, package: Arc<Package>) -> Result<bool> {
    let mut header = Vec::new();
    let mut byte = [0; 1];
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while !header.ends_with(b"\r\n\r\n") {
            anyhow::ensure!(header.len() < 8192, "HTTP header too large");
            socket.read_exact(&mut byte).await?;
            header.push(byte[0]);
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    let line = std::str::from_utf8(&header)?
        .lines()
        .next()
        .context("Missing request")?;
    let mut words = line.split_whitespace();
    let method = words.next().unwrap_or("");
    let path = words.next().unwrap_or("");
    if matches!(method, "GET" | "HEAD") && path == SCRIPT_PATH {
        let script = package.script_for(socket.local_addr().ok().map(|a| a.ip()));
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename={SCRIPT_NAME}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
            script.len()
        );
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            socket.write_all(headers.as_bytes()).await?;
            if method == "GET" {
                socket.write_all(script.as_bytes()).await?;
            }
            Ok::<_, std::io::Error>(())
        })
        .await??;
        return Ok(false);
    }
    if !matches!(method, "GET" | "HEAD") || path != "/" {
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            socket.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            ),
        )
        .await??;
        return Ok(false);
    }
    let headers=format!("HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Disposition: attachment; filename=Classroom-Student-{}.zip\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",package.arch,package.size);
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        socket.write_all(headers.as_bytes()),
    )
    .await??;
    if method == "HEAD" {
        return Ok(false);
    }
    let file = fs::File::open(&package.path)?;
    // Blocking ZIP reads belong to a single producer with bounded chunks.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<std::io::Result<Vec<u8>>>(2);
    let package_guard = Arc::clone(&package);
    tokio::task::spawn_blocking(move || {
        let _package_guard = package_guard;
        let mut file = file;
        loop {
            let mut bytes = vec![0; 64 * 1024];
            match file.read(&mut bytes) {
                Ok(0) => break,
                Ok(n) => {
                    bytes.truncate(n);
                    if tx.blocking_send(Ok(bytes)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.blocking_send(Err(e));
                    break;
                }
            }
        }
    });
    while let Some(bytes) = rx.recv().await {
        tokio::time::timeout(std::time::Duration::from_secs(5), socket.write_all(&bytes?))
            .await??;
        tokio::time::sleep(std::time::Duration::from_millis(8)).await; // ~8 MiB/s per download; four active clients.
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(directory: &Path, valid: bool) -> PathBuf {
        fs::create_dir_all(directory).unwrap();
        let path = directory.join("input.zip");
        let bytes = b"portable-student-fixture";
        let manifest = super::super::dev_update::Manifest {
            format: 1,
            os: "windows".into(),
            arch: "x86_64".into(),
            version: "test".into(),
            root: "ClassroomStudent.exe".into(),
            executable: "ClassroomStudent.exe".into(),
            sha256: if valid {
                format!("{:x}", Sha256::digest(bytes))
            } else {
                "0".repeat(64)
            },
        };
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        zip.start_file("ClassroomStudent.exe", options).unwrap();
        zip.write_all(bytes).unwrap();
        zip.finish().unwrap();
        path
    }
    fn folder(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("classroom-bootstrap-{name}-{}", std::process::id()))
    }
    #[test]
    fn portable_zip_embeds_room_without_identity_and_cleans_bad_checksum() {
        let directory = folder("zip");
        let source = fixture(&directory, true);
        let settings = Bootstrap {
            teacher_ip: Ipv4Addr::new(10, 0, 61, 200),
            control_port: 47800,
            connection_popup: true,
        };
        let out = directory.join("out");
        let package = prepare(&source, &out, &settings, 8080, &AtomicBool::new(false)).unwrap();
        let mut zip = zip::ZipArchive::new(fs::File::open(&package.path).unwrap()).unwrap();
        assert_eq!(zip.len(), 3);
        let mut value = String::new();
        zip.by_name("classroom-bootstrap.toml")
            .unwrap()
            .read_to_string(&mut value)
            .unwrap();
        fs::write(directory.join("classroom-bootstrap.toml"), value).unwrap();
        let saved = Bootstrap::load(&directory).unwrap().unwrap();
        assert_eq!(saved.teacher_ip, settings.teacher_ip);
        assert!(saved.connection_popup);
        let path = package.path.clone();
        drop(zip);
        drop(package);
        assert!(!path.exists());
        let source = fixture(&directory, false);
        assert!(prepare(&source, &out, &settings, 8080, &AtomicBool::new(false)).is_err());
        // Only the installer script copy (owned by the share, not the package) remains.
        let left: Vec<_> = fs::read_dir(out)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, [SCRIPT_NAME]);
        fs::remove_dir_all(directory).unwrap();
    }
    #[tokio::test]
    async fn root_download_is_exact_head_has_no_body_and_other_paths_are_hidden() {
        let directory = folder("http");
        let source = fixture(&directory, true);
        let settings = Bootstrap {
            teacher_ip: Ipv4Addr::LOCALHOST,
            control_port: 47800,
            connection_popup: false,
        };
        let package = prepare(
            &source,
            &directory.join("out"),
            &settings,
            8080,
            &AtomicBool::new(false),
        )
        .unwrap();
        let expected = fs::read(&package.path).unwrap();
        for (request, code, body) in [
            ("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n", "200", true),
            ("HEAD / HTTP/1.1\r\nHost: localhost\r\n\r\n", "200", false),
            ("GET /teacher-endpoint.json HTTP/1.1\r\n\r\n", "404", false),
            ("GET /install.bat/../x HTTP/1.1\r\n\r\n", "404", false),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let item = Arc::clone(&package);
            let worker = tokio::spawn(async move {
                let (socket, _) = listener.accept().await.unwrap();
                serve(socket, item).await.unwrap()
            });
            let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
            socket.write_all(request.as_bytes()).await.unwrap();
            let mut result = Vec::new();
            socket.read_to_end(&mut result).await.unwrap();
            let split = result.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
            let headers = String::from_utf8_lossy(&result[..split]);
            assert!(headers.starts_with(&format!("HTTP/1.1 {code}")));
            assert_eq!(
                &result[split..],
                if body { expected.as_slice() } else { &[] }
            );
            assert_eq!(worker.await.unwrap(), body);
        }
        drop(package);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn the_installer_script_is_self_contained_ascii_crlf_and_names_this_room() {
        let script = install_script(Ipv4Addr::new(10, 0, 55, 200), 8080, 47800, &"ab".repeat(32));
        assert!(script.is_ascii(), "cmd.exe reads it in the console code page");
        assert!(script.starts_with("@echo off\r\n"));
        assert!(!script.replace("\r\n", "").contains('\n'), "every line ends CRLF");
        assert!(script.contains("set \"CLASSROOM_BASE=http://10.0.55.200:8080\""));
        assert!(script.contains("set \"CLASSROOM_TEACHER_IP=10.0.55.200\""));
        assert!(script.contains("set \"CLASSROOM_CONTROL_PORT=47800\""));
        assert!(script.contains(&"ab".repeat(32)));
        for step in [
            "fltmc",                   // asks for administrator when it is not
            "Start-Process -FilePath '%SELF%' -Verb RunAs",
            "/quiet",                  // no pause for remote execution
            "Invoke-WebRequest",       // downloads the ZIP
            "Get-FileHash",            // verifies it
            "Expand-Archive",          // unpacks it
            "ClassroomGuardService.exe",
            "'--install', '--teacher-ip'",
            "exit /b %RESULT%",
        ] {
            assert!(script.contains(step) || script.replace("\r\n", "\n").contains(step), "missing {step}");
        }
        // The PowerShell part must start after the last `exit /b`, never be parsed by cmd.
        let marker = script.rfind("#PS-BEGIN").unwrap();
        assert!(script.rfind("exit /b %RESULT%").unwrap() < marker);
        // Port 80 needs no port in the URL.
        assert!(install_script(Ipv4Addr::new(10, 0, 55, 200), 80, 47800, "x").contains("CLASSROOM_BASE=http://10.0.55.200\""));
        // No placeholder is left unreplaced.
        for placeholder in ["@BASE@", "@SHA@", "@IP@", "@CONTROL@", "@PORTSUFFIX@"] {
            assert!(!script.contains(placeholder), "{placeholder} left in the script");
        }
        // The address can be overridden on the command line without editing the file.
        assert!(script.contains("/teacher"));
        assert!(script.contains("set \"CLASSROOM_BASE=http://%~2:8080\""));
        assert!(install_script(Ipv4Addr::new(10, 0, 55, 200), 80, 47800, "x").contains("CLASSROOM_BASE=http://%~2\""));
    }

    #[tokio::test]
    async fn the_script_is_served_as_a_download_next_to_the_zip() {
        let directory = folder("script");
        let source = fixture(&directory, true);
        let settings = Bootstrap {
            teacher_ip: Ipv4Addr::LOCALHOST,
            control_port: 47800,
            connection_popup: false,
        };
        let package = prepare(&source, &directory.join("out"), &settings, 8080, &AtomicBool::new(false)).unwrap();
        // The script names the checksum of the very ZIP it will fetch.
        let zip_hash = format!("{:x}", Sha256::digest(fs::read(&package.path).unwrap()));
        let text = package.script_for(None);
        assert!(text.contains(&zip_hash));
        // The Teacher also gets it as a file.
        assert_eq!(fs::read(&package.script_path).unwrap(), text.as_bytes());
        // It follows the address the Student used, not a fixed one.
        let other = package.script_for(Some(std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 7, 9))));
        assert!(other.contains("CLASSROOM_TEACHER_IP=192.168.7.9") && other.contains("http://192.168.7.9:8080"));
        assert!(!other.contains("127.0.0.1"));
        for (method, with_body) in [("GET", true), ("HEAD", false)] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let item = Arc::clone(&package);
            let worker = tokio::spawn(async move {
                let (socket, _) = listener.accept().await.unwrap();
                serve(socket, item).await.unwrap()
            });
            let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
            socket
                .write_all(format!("{method} /install.bat HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
                .await
                .unwrap();
            let mut result = Vec::new();
            socket.read_to_end(&mut result).await.unwrap();
            let split = result.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4;
            let headers = String::from_utf8_lossy(&result[..split]).to_string();
            assert!(headers.starts_with("HTTP/1.1 200"));
            assert!(headers.contains("filename=Install-Classroom-Student.bat"));
            // Reached over loopback, so the script names the address the share was started on.
            let expected = package.script_for(None);
            assert!(headers.contains(&format!("Content-Length: {}", expected.len())));
            assert_eq!(&result[split..], if with_body { expected.as_bytes() } else { &[] });
            assert!(!worker.await.unwrap(), "a script fetch is not counted as a ZIP download");
        }
        let zip_path = package.path.clone();
        drop(package);
        assert!(!zip_path.exists(), "the ZIP goes with its package");
        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn a_student_reaching_a_new_teacher_address_gets_a_package_and_script_for_it() {
        let directory = folder("move");
        let source = fixture(&directory, true);
        let refresher = Arc::new(Refresher {
            source,
            directory: directory.join("out"),
            port: 8080,
            control_port: 47800,
            popup: false,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        let first = refresher.build(Ipv4Addr::new(10, 0, 55, 200)).unwrap();
        assert_eq!(first.ip, Ipv4Addr::new(10, 0, 55, 200));
        let current = tokio::sync::Mutex::new(Arc::clone(&first));
        let status = Mutex::new(serde_json::json!({"url":"http://10.0.55.200:8080/"}));
        // Same address, loopback or unknown: nothing is rebuilt.
        for local in [
            Some(std::net::IpAddr::V4(Ipv4Addr::new(10, 0, 55, 200))),
            Some(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST)),
            None,
        ] {
            let kept = refresher.package_for(&current, local, &status).await;
            assert!(Arc::ptr_eq(&kept, &first));
        }
        // The Teacher is now on Wi-Fi: a Student arriving there gets the new address everywhere.
        let wifi = Ipv4Addr::new(192, 168, 1, 77);
        let moved = refresher
            .package_for(&current, Some(std::net::IpAddr::V4(wifi)), &status)
            .await;
        assert_eq!(moved.ip, wifi);
        assert!(moved.script_for(None).contains("CLASSROOM_TEACHER_IP=192.168.1.77"));
        assert!(moved.script_for(None).contains("http://192.168.1.77:8080"));
        let mut zip = zip::ZipArchive::new(fs::File::open(&moved.path).unwrap()).unwrap();
        let mut toml = String::new();
        zip.by_name("classroom-bootstrap.toml").unwrap().read_to_string(&mut toml).unwrap();
        assert!(toml.contains("192.168.1.77"), "the ZIP names the new address too: {toml}");
        assert_eq!(status.lock().unwrap()["url"], "http://192.168.1.77:8080/");
        assert_eq!(status.lock().unwrap()["script_url"], "http://192.168.1.77:8080/install.bat");
        // The old package going away must not take the shared script copy with it.
        drop(first);
        assert!(moved.script_path.exists());
        assert_eq!(fs::read(&moved.script_path).unwrap(), moved.script_for(None).as_bytes());
        // And back again.
        let back = refresher
            .package_for(&current, Some(std::net::IpAddr::V4(Ipv4Addr::new(10, 0, 55, 200))), &status)
            .await;
        assert_eq!(back.ip, Ipv4Addr::new(10, 0, 55, 200));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn the_script_corrects_the_room_address_after_unpacking() {
        let script = install_script(Ipv4Addr::new(10, 0, 55, 200), 8080, 47800, "x");
        assert!(script.contains("classroom-bootstrap.toml"));
        assert!(script.contains("'teacher_ip = \"' + $env:CLASSROOM_TEACHER_IP"));
        assert!(script.contains("'control_port = ' + $env:CLASSROOM_CONTROL_PORT"));
    }
}
