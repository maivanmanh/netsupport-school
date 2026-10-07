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
        let listener = std::net::TcpListener::bind(SocketAddrV4::new(ip, port)).context(
            "Cannot open Student download port; choose a free port or allow it in the firewall",
        )?;
        listener.set_nonblocking(true)?;
        let listener = TcpListener::from_std(listener)?;
        let state = Arc::new(Mutex::new(
            serde_json::json!({"phase":"preparing","url":url(ip,port)}),
        ));
        let status = Arc::clone(&state);
        let (stop, mut stopped) = watch::channel(false);
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel_prepare = Arc::clone(&cancelled);
        let worker = tokio::spawn(async move {
            let preparation = tokio::task::spawn_blocking(move || {
                prepare(
                    &source,
                    &directory,
                    &Bootstrap {
                        teacher_ip: ip,
                        control_port,
                        connection_popup: popup,
                    },
                    &cancel_prepare,
                )
            });
            // Preparation is bounded; its result owns cleanup even when this task is dropped.
            let package = match preparation.await {
                Ok(Ok(package)) => package,
                result => {
                    *status.lock().unwrap() =
                        serde_json::json!({"phase":"error","error":format!("{result:?}")});
                    return;
                }
            };
            *status.lock().unwrap() = serde_json::json!({"phase":"sharing","url":url(ip,port),"size":package.size,"downloads":0,"arch":package.arch});
            tracing::info!(%ip,port,bytes=package.size,"Student ZIP sharing ready");
            let slots = Arc::new(Semaphore::new(4));
            let mut tasks = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = stopped.changed() => break,
                    accepted = listener.accept() => {
                        let Ok((socket,peer)) = accepted else { break };
                        if tasks.len() < 48 {
                            let permit = Arc::clone(&slots).try_acquire_owned();
                            let package = Arc::clone(&package);
                            let status = Arc::clone(&status);
                            tasks.spawn(async move {
                                let Ok(_permit) = permit else {
                                    let mut socket=socket;
                                    let body=b"<html><head><meta http-equiv=refresh content=3></head><body>Classroom: waiting for a download slot. Retrying...</body></html>";
                                    let response=format!("HTTP/1.1 503 Service Unavailable\r\nRetry-After: 3\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
                                    let _=tokio::time::timeout(std::time::Duration::from_secs(1),async {socket.write_all(response.as_bytes()).await?;socket.write_all(body).await}).await;
                                    return;
                                };
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
        });
        Ok(Self {
            stop,
            worker,
            cancelled,
            state,
        })
    }
}
impl Drop for Share {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.stop.send(true);
        self.worker.abort(); // release listener/download tasks immediately, including preparation
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
#[derive(Debug)]
struct Package {
    path: PathBuf,
    size: u64,
    arch: String,
}
impl Drop for Package {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
fn prepare(
    source: &Path,
    directory: &Path,
    settings: &Bootstrap,
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
        Ok(Arc::new(Package {
            path: path.clone(),
            size,
            arch: manifest.arch,
        }))
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
        let package = prepare(&source, &out, &settings, &AtomicBool::new(false)).unwrap();
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
        assert!(prepare(&source, &out, &settings, &AtomicBool::new(false)).is_err());
        assert_eq!(fs::read_dir(out).unwrap().count(), 0);
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
            &AtomicBool::new(false),
        )
        .unwrap();
        let expected = fs::read(&package.path).unwrap();
        for (request, code, body) in [
            ("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n", "200", true),
            ("HEAD / HTTP/1.1\r\nHost: localhost\r\n\r\n", "200", false),
            ("GET /teacher-endpoint.json HTTP/1.1\r\n\r\n", "404", false),
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
}
