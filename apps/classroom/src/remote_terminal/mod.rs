//! Teacher-authorized, non-elevated native PTY sessions. No manual pairing.
pub mod compat;
pub mod teacher;
mod trust;
#[cfg(windows)]
mod windows_worker;
mod wire;
pub mod worker;

pub fn available() -> bool {
    cfg!(any(windows, target_os = "macos"))
}

pub struct Worker {
    #[cfg(windows)]
    process: windows_worker::Process,
    #[cfg(not(windows))]
    process: std::process::Child,
}
impl Worker {
    pub fn finished(&mut self) -> bool {
        #[cfg(windows)]
        {
            self.process.finished()
        }
        #[cfg(not(windows))]
        {
            self.process.try_wait().is_ok_and(|v| v.is_some())
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        #[cfg(not(windows))]
        {
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }
}
pub fn start(
    address: std::net::SocketAddrV4,
    student: String,
    session: String,
    compatibility: bool,
    authorization: &str,
) -> anyhow::Result<Worker> {
    let policy: trust::Policy = serde_json::from_str(authorization)?;
    policy.validate(address.ip())?;
    anyhow::ensure!(
        session.len() == 32 && session.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid terminal session ID"
    );
    let context = worker::Context {
        address,
        student,
        session,
        compatibility,
        policy,
    };
    let root = crate::launcher::settings_path()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .ok_or_else(|| anyhow::anyhow!("Student data directory unavailable"))?
        .join("terminal-workers");
    std::fs::create_dir_all(&root)?;
    let path = root.join(format!("{}.json", context.session));
    trust::private_write(&path, &serde_json::to_vec(&context)?)?;
    #[cfg(windows)]
    {
        let result = windows_worker::spawn(&path).map(|process| Worker { process });
        if result.is_err() {
            let _ = std::fs::remove_file(&path);
        }
        result
    }
    #[cfg(not(windows))]
    {
        let result = std::process::Command::new(std::env::current_exe()?)
            .arg("--terminal-worker")
            .arg(&path)
            .spawn()
            .map(|process| Worker { process })
            .map_err(Into::into);
        if result.is_err() {
            let _ = std::fs::remove_file(&path);
        }
        result
    }
}
