#![allow(unsafe_code)] // Documented POSIX identity and owned process-group FFI.
use super::{
    trust,
    wire::{self, Frame},
};
use anyhow::{ensure, Context as _, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::SocketAddrV4,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{net::TcpStream, sync::mpsc};
use tokio_rustls::{
    rustls::{
        pki_types::{CertificateDer, ServerName},
        ClientConfig, RootCertStore,
    },
    TlsConnector,
};

#[derive(Serialize, Deserialize)]
pub struct Context {
    pub address: SocketAddrV4,
    pub student: String,
    pub session: String,
    #[serde(default = "default_compatibility")]
    pub compatibility: bool,
    pub policy: trust::Policy,
}
fn default_compatibility() -> bool {
    true
}
struct Kill {
    child: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    #[cfg(unix)]
    fd: std::os::fd::OwnedFd,
    #[cfg(windows)]
    _job: super::windows_worker::Job,
}
struct ChildGuard(Box<dyn portable_pty::Child + Send + Sync>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Drop for Kill {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // SAFETY: the owned descriptor is this session's PTY master.
            unsafe {
                let group = libc::tcgetpgrp(self.fd.as_raw_fd());
                if group > 0 {
                    libc::kill(-group, libc::SIGHUP);
                }
            }
        }
        let _ = self.child.kill();
    }
}
pub async fn run(path: &Path) -> Result<()> {
    let context: Context = serde_json::from_slice(&std::fs::read(path)?)?;
    std::fs::remove_file(path)?;
    #[cfg(windows)]
    ensure!(
        !crate::guard::windows::is_elevated(),
        "Remote shell refuses an elevated token"
    );
    #[cfg(unix)]
    {
        // SAFETY: geteuid is a side-effect-free OS identity query.
        ensure!(unsafe { libc::geteuid() } != 0, "Remote shell refuses root");
    }
    let policy = &context.policy;
    policy.validate(context.address.ip())?;
    let root = crate::launcher::settings_path()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .context("Student audit directory unavailable")?;
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(
        STANDARD.decode(&policy.teacher_certificate)?,
    ))?;
    let connector = TlsConnector::from(Arc::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ));
    // Authenticate the pinned Teacher and obtain its session authorization
    // before starting a shell or running any user initialization script.
    let first = tokio::time::timeout(
        Duration::from_secs(8),
        connect(&context, &policy, &connector),
    )
    .await??;
    trust::audit(
        &root,
        &context.session,
        "shell_authorized",
        json!({"student":context.student,"teacher":context.address.ip().to_string()}),
    )?;
    let mut initial = Some(first);
    let (tx, mut output) = mpsc::channel::<Frame>(32);
    let (input, mut rx) = mpsc::channel::<Frame>(32);
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let compatibility = context.compatibility;
    let allow_desktop_open = policy.allow_desktop_open;
    let helper_root = root.join("terminal-workers");
    let pty = std::thread::spawn(move || -> Result<()> {
        #[cfg(windows)]
        super::windows_worker::reset_ctrl_c_inheritance()?;
        #[cfg(not(windows))]
        let _ = (compatibility, helper_root, allow_desktop_open);
        #[cfg(windows)]
        let helpers = if compatibility {
            Some(super::compat::Helpers::install(&helper_root)?)
        } else {
            None
        };
        let pair = native_pty_system().openpty(PtySize {
            rows: 30,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let shell = if cfg!(windows) {
            PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
                .join("System32")
                .join("cmd.exe")
                .to_string_lossy()
                .into_owned()
        } else if cfg!(target_os = "linux") {
            // The user's login shell, falling back to bash (always present on Ubuntu).
            std::env::var("SHELL")
                .ok()
                .filter(|shell| std::path::Path::new(shell).is_file())
                .unwrap_or_else(|| "/bin/bash".into())
        } else {
            "/bin/zsh".into()
        };
        let cwd = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(PathBuf::from)
            .context("Student home unavailable")?;
        let mut command = CommandBuilder::new(&shell);
        if cfg!(windows) {
            command.arg("/Q");
            #[cfg(windows)]
            if let Some(helpers) = &helpers {
                let mut paths: Vec<_> =
                    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
                paths.push(helpers.0.clone());
                command.env("PATH", std::env::join_paths(paths)?);
                command.env(
                    "CLASSROOM_TERMINAL_EXPORT",
                    helpers.0.join("export-apply.cmd"),
                );
                if allow_desktop_open {
                    command.env("CLASSROOM_TERMINAL_ALLOW_OPEN", "1");
                }
            }
        } else {
            command.args(["-l", "-i"]);
            command.env("TERM", "xterm-256color");
            command.env("COLORTERM", "truecolor");
        }
        command.cwd(&cwd);
        let mut owned_child = ChildGuard(pair.slave.spawn_command(command)?);
        let child = &mut owned_child.0;
        #[cfg(windows)]
        let job = match child
            .process_id()
            .context("PTY process ID unavailable")
            .and_then(super::windows_worker::Job::for_pid)
        {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        #[cfg(unix)]
        let fd = {
            use std::os::fd::FromRawFd;
            let raw = pair
                .master
                .as_raw_fd()
                .context("PTY descriptor unavailable")?;
            // SAFETY: transfer this new duplicate exactly once to OwnedFd.
            let duplicate = unsafe { libc::dup(raw) };
            ensure!(duplicate >= 0, "Cannot duplicate PTY shutdown descriptor");
            unsafe { std::os::fd::OwnedFd::from_raw_fd(duplicate) }
        };
        let kill = Kill {
            child: child.clone_killer(),
            #[cfg(unix)]
            fd,
            #[cfg(windows)]
            _job: job,
        };
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;
        if ready_tx.send((json!({"state":"ready","os":if cfg!(windows){"windows"}else if cfg!(target_os = "linux"){"linux"}else{"macos"},"shell":shell,"cwd":cwd,"elevated":false}),kill)).is_err(){let _=child.kill();let _=child.wait();return Ok(());}
        let reader_tx = tx.clone();
        let reader_task = std::thread::spawn(move || {
            let mut seq = 0;
            let mut bytes = [0; 8192];
            while let Ok(n) = reader.read(&mut bytes) {
                if n == 0 {
                    break;
                }
                seq += 1;
                if reader_tx
                    .blocking_send(Frame::new(wire::DATA, seq, bytes[..n].to_vec()))
                    .is_err()
                {
                    break;
                }
            }
            let _ = reader_tx.blocking_send(Frame::new(wire::EXIT, 0, vec![]));
        });
        // PTY input/resize is serial and off the Tokio/network thread.
        let outcome = (|| -> Result<()> {
            loop {
                let f = match rx.try_recv() {
                    Ok(frame) => frame,
                    Err(mpsc::error::TryRecvError::Disconnected) => break,
                    Err(mpsc::error::TryRecvError::Empty) => {
                        if child.try_wait()?.is_some() {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                };
                match f.kind {
                    wire::INPUT => {
                        writer.write_all(&f.data)?;
                        writer.flush()?;
                    }
                    wire::RESIZE => {
                        ensure!(f.data.len() == 4, "Invalid terminal size");
                        let cols = u16::from_be_bytes([f.data[0], f.data[1]]).clamp(2, 500);
                        let rows = u16::from_be_bytes([f.data[2], f.data[3]]).clamp(2, 300);
                        pair.master.resize(PtySize {
                            rows,
                            cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        })?;
                    }
                    wire::CLOSE => break,
                    _ => anyhow::bail!("Invalid PTY input"),
                }
            }
            Ok(())
        })();
        #[cfg(unix)]
        if let Some(pid) = child.process_id() {
            // SAFETY: target is the owned PTY shell's process group.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGHUP);
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        drop(writer);
        drop(pair.master);
        // Closing the async receiver releases a reader blocked by backpressure.
        let _ = tx.blocking_send(Frame::new(wire::EXIT, 0, vec![]));
        let _ = reader_task.join();
        outcome
    });
    let (ready, kill) = tokio::time::timeout(Duration::from_secs(10), ready_rx).await??;
    trust::audit(
        &root,
        &context.session,
        "shell_open",
        json!({"os":ready["os"],"elevated":false}),
    )?;
    let mut pending = VecDeque::<Frame>::new();
    let mut last_input = 0;
    let mut disconnected = Instant::now();
    let mut ended = false;
    while !ended && disconnected.elapsed() < wire::GRACE {
        let attempt=async {
            let mut tls=if let Some(first)=initial.take(){first}else{connect(&context,&policy,&connector).await?};
            wire::write(&mut tls,&Frame::new(wire::READY,0,serde_json::to_vec(&ready)?)).await?;
            let (mut reader,mut writer)=tokio::io::split(tls);
            let (tx,mut frames)=mpsc::channel(8);
            let reader_task=tokio::spawn(async move {while let Ok(f)=wire::read(&mut reader).await{if tx.send(f).await.is_err(){break;}}});
            let outcome=async {
                for f in &pending{wire::write(&mut writer,f).await?;}
                disconnected=Instant::now();
                loop {
                    tokio::select! {
                        f=frames.recv()=> {
                            let f=f.context("Teacher terminal disconnected")?;
                            disconnected=Instant::now();
                            match f.kind {
                                wire::ACK=>pending.retain(|p|p.seq>f.seq),
                                wire::INPUT|wire::RESIZE=>{
                                    if f.seq>last_input {
                                        ensure!(f.seq==last_input+1,"Terminal input sequence gap");
                                        tokio::time::timeout(Duration::from_secs(2),input.send(f.clone())).await??;last_input=f.seq;
                                    }
                                    wire::write(&mut writer,&Frame::new(wire::ACK,f.seq,vec![])).await?;
                                }
                                wire::CLOSE=>{ended=true;break;}
                                wire::PING=>wire::write(&mut writer,&Frame::new(wire::PING,0,vec![])).await?,
                                _=>anyhow::bail!("Invalid Teacher terminal frame"),
                            }
                        }
                        f=output.recv(),if pending.len()<32=>{
                            let f=f.context("PTY ended")?;
                            if f.kind==wire::EXIT{wire::write(&mut writer,&f).await?;ended=true;break;}
                            pending.push_back(f.clone());wire::write(&mut writer,&f).await?;
                        }
                        _=tokio::time::sleep(Duration::from_secs(4))=>anyhow::bail!("Terminal heartbeat timeout"),
                    }
                }
                Ok::<(),anyhow::Error>(())
            }.await;
            reader_task.abort();outcome
        }.await;
        if let Err(error) = attempt {
            let _ = trust::audit(
                &root,
                &context.session,
                "reconnect",
                json!({"error":error.to_string()}),
            );
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }
    output.close();
    drop(output);
    drop(kill);
    drop(input);
    let result = tokio::task::spawn_blocking(move || {
        pty.join()
            .map_err(|_| anyhow::anyhow!("PTY worker panicked"))?
    })
    .await?;
    trust::audit(
        &root,
        &context.session,
        "shell_close",
        json!({"error":result.as_ref().err().map(ToString::to_string)}),
    )?;
    result
}

async fn connect(
    context: &Context,
    policy: &trust::Policy,
    connector: &TlsConnector,
) -> Result<tokio_rustls::client::TlsStream<TcpStream>> {
    use prost::Message;
    use tokio::io::AsyncWriteExt;
    let mut stream =
        tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(context.address)).await??;
    stream.set_nodelay(true)?;
    let hello = protocol::ToolPacket {
        version: 1,
        action: "terminal_connect".into(),
        student: context.student.clone(),
        id: context.session.clone(),
        ..Default::default()
    };
    let bytes = hello.encode_to_vec();
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .await?;
    stream.write_all(&bytes).await?;
    let mut tls = tokio::time::timeout(
        Duration::from_secs(5),
        connector.connect(ServerName::try_from("classroom.local")?, stream),
    )
    .await??;
    wire::write(
        &mut tls,
        &Frame::new(wire::AUTH, 0, policy.secret.as_bytes().to_vec()),
    )
    .await?;
    let authorization =
        tokio::time::timeout(Duration::from_secs(3), wire::read(&mut tls)).await??;
    ensure!(
        authorization.kind == wire::AUTH,
        "Teacher did not authorize terminal session"
    );
    Ok(tls)
}
