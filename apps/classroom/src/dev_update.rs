//! Development-only self-update. Bulk transfer is owned by `classroom_tools`;
//! this module validates packages and supervises a same-volume swap after exit.
//! No production signing/trust or OS privacy-policy bypass is provided.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};

pub const MAX_PACKAGE: u64 = 512 * 1024 * 1024;
const MAX_EXPANDED: u64 = 1024 * 1024 * 1024;
const RECEIPT_ENV: &str = "CLASSROOM_DEV_UPDATE_RECEIPT";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Manifest {
    pub format: u32,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub root: String,
    pub executable: String,
    pub sha256: String,
}
impl Manifest {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(self.format == 1, "Unsupported dev-update package format");
        anyhow::ensure!(
            matches!(self.os.as_str(), "windows" | "macos"),
            "Unsupported package OS"
        );
        anyhow::ensure!(
            matches!(self.arch.as_str(), "x86_64" | "aarch64"),
            "Unsupported package architecture"
        );
        anyhow::ensure!(
            !self.version.is_empty() && self.version.len() <= 80,
            "Invalid update version"
        );
        safe_relative(&self.root)?;
        safe_relative(&self.executable)?;
        anyhow::ensure!(
            Path::new(&self.root).components().count() == 1,
            "Package root must be one item"
        );
        anyhow::ensure!(
            Path::new(&self.executable).starts_with(&self.root),
            "Executable outside package root"
        );
        anyhow::ensure!(
            self.sha256.len() == 64 && self.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid executable hash"
        );
        if self.os == "macos"
            && Path::new(&self.root)
                .extension()
                .is_some_and(|e| e == "app")
        {
            anyhow::ensure!(
                self.executable == format!("{}/Contents/MacOS/ClassroomStudent", self.root),
                "Unexpected Student bundle executable"
            );
        } else {
            anyhow::ensure!(
                self.root == self.executable,
                "Raw package must contain one executable"
            );
        }
        Ok(())
    }
}
fn safe_relative(name: &str) -> Result<()> {
    anyhow::ensure!(
        !name.is_empty() && !name.contains(['\\', ':']),
        "Unsafe package path"
    );
    anyhow::ensure!(
        Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_))),
        "Unsafe package path"
    );
    Ok(())
}
pub fn manifest(path: &Path) -> Result<Manifest> {
    anyhow::ensure!(
        fs::metadata(path)?.len() <= MAX_PACKAGE,
        "Dev-update package exceeds 512 MiB"
    );
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
    let entry = zip.by_name("manifest.json")?;
    anyhow::ensure!(entry.size() <= 8192, "Update manifest too large");
    let m: Manifest = serde_json::from_reader(entry.take(8193))?;
    m.validate()?;
    Ok(m)
}
fn hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut bytes = vec![0_u8; 64 * 1024];
    loop {
        let n = file.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        digest.update(&bytes[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Plan {
    pub id: String,
    pub token: String,
    pub manifest: Manifest,
    pub target: PathBuf,
    pub staged: PathBuf,
    pub backup: PathBuf,
    pub receipt: PathBuf,
    pub executable_suffix: PathBuf,
    pub parent_pid: u32,
    pub args: Vec<String>,
}
pub fn capabilities() -> serde_json::Value {
    let receipt = std::env::var_os(RECEIPT_ENV)
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .filter(|r| {
            r["outcome"] == "rolled_back"
                || std::env::current_exe()
                    .ok()
                    .and_then(|p| hash(&p).ok())
                    .is_some_and(|h| r["sha256"] == h)
        });
    let desktop_supported = u8::from(cfg!(any(windows, target_os = "macos")));
    serde_json::json!({"dev_update":desktop_supported,"connection_popup":desktop_supported,"fs":1,"launch":desktop_supported,"power":desktop_supported,"os":std::env::consts::OS,"arch":std::env::consts::ARCH,
        "build":env!("CLASSROOM_BUILD_ID"),"pid":std::process::id(),"last_update":receipt})
}
pub fn connected() {
    if let Some(path) = std::env::var_os(RECEIPT_ENV) {
        let _ = fs::write(
            PathBuf::from(path).with_extension("connected"),
            b"connected",
        );
    }
}
fn install_target(exe: &Path) -> (PathBuf, PathBuf) {
    if cfg!(target_os = "macos") {
        if let Some(bundle) = exe
            .ancestors()
            .find(|p| p.extension().is_some_and(|s| s == "app"))
        {
            return (
                bundle.to_owned(),
                exe.strip_prefix(bundle).unwrap().to_owned(),
            );
        }
    }
    (exe.to_owned(), PathBuf::new())
}
/// Extract, hash, and probe the new executable BEFORE asking the old process to exit.
pub fn prepare(package: &Path, id: &str, token: &str, cancel: &AtomicBool) -> Result<PathBuf> {
    let m = manifest(package)?;
    anyhow::ensure!(
        m.os == std::env::consts::OS && m.arch == std::env::consts::ARCH,
        "Package OS/architecture does not match this Student"
    );
    let current = std::env::current_exe()?.canonicalize()?;
    let (target, suffix) = install_target(&current);
    anyhow::ensure!(
        Path::new(&m.root).extension().is_some_and(|e| e == "app")
            == target.extension().is_some_and(|s| s == "app"),
        "Bundle/raw package does not match Student installation"
    );
    let parent = target
        .parent()
        .context("Student installation folder missing")?;
    let stage = parent.join(format!(".classroom-update-{id}"));
    fs::create_dir(&stage).context("Could not create update staging folder beside Student")?;
    let result = (|| {
        let unpack = stage.join("payload");
        fs::create_dir(&unpack)?;
        extract(package, &unpack, cancel)?;
        let executable = unpack.join(&m.executable);
        anyhow::ensure!(
            hash(&executable)? == m.sha256,
            "Update executable checksum mismatch"
        );
        // No tray, capture, network or config initialization in probe mode.
        let probe_path = stage.join("probe.json");
        let mut child = Command::new(&executable)
            .arg("--dev-update-probe")
            .stdout(fs::File::create(&probe_path)?)
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("New Student could not run its compatibility probe")?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait()? {
                anyhow::ensure!(
                    status.success(),
                    "New Student rejected update compatibility probe"
                );
                let bytes = fs::read(&probe_path)?;
                anyhow::ensure!(bytes.len() <= 8192, "Probe output too large");
                let probe: serde_json::Value = serde_json::from_slice(&bytes)
                    .context("New binary is not an updater-enabled Student")?;
                anyhow::ensure!(
                    probe["dev_update"] == 1
                        && probe["student"] == true
                        && probe["os"] == m.os
                        && probe["arch"] == m.arch,
                    "New binary is not a compatible Student"
                );
                break;
            }
            if Instant::now() >= deadline || cancel.load(std::sync::atomic::Ordering::Acquire) {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("New Student probe timed out or was cancelled");
            }
            thread::sleep(Duration::from_millis(25));
        }
        let plan = Plan {
            id: id.into(),
            token: token.into(),
            manifest: m.clone(),
            target,
            staged: unpack.join(&m.root),
            backup: stage.join("previous"),
            receipt: stage.join("receipt.json"),
            executable_suffix: suffix,
            parent_pid: std::process::id(),
            args: std::env::args().skip(1).collect(),
        };
        let plan_path = stage.join("plan.json");
        fs::write(&plan_path, serde_json::to_vec(&plan)?)?;
        prepare_helper(&plan_path)?;
        Ok(plan_path)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}
fn extract(package: &Path, output: &Path, cancel: &AtomicBool) -> Result<()> {
    let mut archive = zip::ZipArchive::new(fs::File::open(package)?)?;
    anyhow::ensure!(archive.len() <= 10_000, "Too many update entries");
    let mut expanded = 0_u64;
    for i in 0..archive.len() {
        anyhow::ensure!(
            !cancel.load(std::sync::atomic::Ordering::Acquire),
            "Update cancelled"
        );
        let mut entry = archive.by_index(i)?;
        safe_relative(entry.name().trim_end_matches('/'))?;
        let path = entry.enclosed_name().context("Unsafe ZIP entry")?;
        if entry.name() == "manifest.json" {
            continue;
        }
        anyhow::ensure!(
            entry
                .unix_mode()
                .is_none_or(|mode| mode & 0o170_000 != 0o120_000),
            "Symlinks not allowed in dev-update package"
        );
        expanded = expanded
            .checked_add(entry.size())
            .context("ZIP size overflow")?;
        anyhow::ensure!(
            expanded <= MAX_EXPANDED,
            "Update exceeds expanded 1 GiB limit"
        );
        let destination = output.join(path);
        if entry.is_dir() {
            fs::create_dir_all(destination)?;
            continue;
        }
        fs::create_dir_all(destination.parent().context("ZIP parent missing")?)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        let written = std::io::copy(&mut entry.by_ref().take(MAX_EXPANDED + 1), &mut file)?;
        anyhow::ensure!(written == entry.size(), "ZIP entry size mismatch");
        file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = if entry.unix_mode().unwrap_or(0) & 0o111 != 0 {
                0o755
            } else {
                0o644
            };
            fs::set_permissions(&destination, fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}
fn helper_path(plan: &Path) -> PathBuf {
    plan.with_file_name(if cfg!(windows) {
        "ClassroomSwap.exe"
    } else {
        "ClassroomSwap"
    })
}
fn prepare_helper(plan: &Path) -> Result<()> {
    let helper = helper_path(plan);
    fs::copy(std::env::current_exe()?, &helper)?;
    let mut child = Command::new(&helper)
        .arg("--dev-update-probe")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Could not start normal-user swap helper before update commit")?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait()? {
            anyhow::ensure!(status.success(), "Swap helper probe failed");
            return Ok(());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Swap helper probe timed out");
        }
        thread::sleep(Duration::from_millis(25));
    }
}
pub fn launch_worker(plan: &Path) -> Result<()> {
    let helper = helper_path(plan);
    let log = fs::File::create(plan.with_file_name("updater.log"))?;
    Command::new(helper)
        .arg("--dev-update-worker")
        .arg(plan)
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()?;
    Ok(())
}
fn process_alive(pid: u32) -> bool {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    system.process(pid).is_some()
}
fn launch(plan: &Plan, updated: bool) -> Result<std::process::Child> {
    let exe = plan.target.join(&plan.executable_suffix);
    // Joining an empty suffix adds a trailing slash to a raw executable.
    let exe = if plan.executable_suffix.as_os_str().is_empty() {
        plan.target.clone()
    } else {
        exe
    };
    let mut command = Command::new(exe);
    command.args(&plan.args).stdin(std::process::Stdio::null());
    if updated {
        command.env(RECEIPT_ENV, &plan.receipt);
    } else {
        command.env_remove(RECEIPT_ENV);
    }
    Ok(command.spawn()?)
}
fn copy_logs(source: &Path, dest: &Path) -> Result<()> {
    if !source.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            copy_logs(&entry.path(), &dest.join(entry.file_name()))?;
        } else if entry.file_type()?.is_file() {
            fs::copy(entry.path(), dest.join(entry.file_name()))?;
        }
    }
    Ok(())
}
fn swap(plan: &Plan) -> Result<()> {
    fs::rename(&plan.target, &plan.backup).context("Could not back up old Student")?;
    if let Err(error) = fs::rename(&plan.staged, &plan.target) {
        fs::rename(&plan.backup, &plan.target).context("Restore after failed swap")?;
        return Err(error).context("Could not install new Student");
    }
    Ok(())
}
fn rollback(plan: &Plan) -> Result<()> {
    fs::rename(&plan.target, &plan.staged).context("Could not move failed update aside")?;
    fs::rename(&plan.backup, &plan.target).context("Could not restore old Student")?;
    Ok(())
}
fn clean_previous_successes(plan: &Plan) {
    let Some(parent) = plan.target.parent() else {
        return;
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|s| s.strip_prefix(".classroom-update-"))
        else {
            continue;
        };
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) || id == plan.id {
            continue;
        }
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            let old = fs::read(path.join("plan.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<Plan>(&b).ok());
            if old.is_some_and(|old| {
                old.target == plan.target && old.receipt.with_extension("connected").exists()
            }) {
                // Only completed transactions for this exact installation. The
                // Current receipt and all failed runs survive.
                let _ = fs::remove_dir_all(path);
            }
        }
    }
}
pub fn worker(path: &Path) -> Result<()> {
    let plan: Plan = serde_json::from_slice(&fs::read(path)?)?;
    let deadline = Instant::now() + Duration::from_secs(60);
    while process_alive(plan.parent_pid) {
        anyhow::ensure!(
            Instant::now() < deadline,
            "Old Student did not exit; left installation unchanged"
        );
        thread::sleep(Duration::from_millis(100));
    }
    if let Err(error) = swap(&plan) {
        if plan.target.exists() {
            let _ = launch(&plan, false);
        }
        return Err(error);
    }
    let result = (|| -> Result<()> {
        if plan.target.extension().is_some_and(|s| s == "app") {
            copy_logs(
                &plan.backup.join("Contents/MacOS/logs"),
                &plan.target.join("Contents/MacOS/logs"),
            )?;
        }
        let receipt = serde_json::json!({"id":plan.id,"token":plan.token,"version":plan.manifest.version,
            "sha256":plan.manifest.sha256,"outcome":"installed"});
        fs::write(&plan.receipt, serde_json::to_vec(&receipt)?)?;
        let mut child = launch(&plan, true)?;
        let deadline = Instant::now() + Duration::from_secs(90);
        let connected = plan.receipt.with_extension("connected");
        loop {
            if connected.exists() {
                // Reconnection commits the update. Until this point the backup
                // is needed for rollback; afterwards retain no obsolete app.
                let cleanup = if plan.backup.is_dir() {
                    fs::remove_dir_all(&plan.backup)
                } else {
                    fs::remove_file(&plan.backup)
                };
                if let Err(error) = cleanup {
                    eprintln!("Update {} backup cleanup failed: {error}", plan.id);
                }
                clean_previous_successes(&plan);
                println!(
                    "Update {} connected; obsolete build cleanup attempted",
                    plan.id
                );
                return Ok(());
            }
            if child.try_wait()?.is_some() || Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("New Student failed to reconnect within 90 seconds");
            }
            thread::sleep(Duration::from_millis(100));
        }
    })();
    if let Err(error) = result {
        eprintln!("Update failed: {error:#}; restoring previous build");
        rollback(&plan)?;
        fs::write(
            &plan.receipt,
            serde_json::to_vec(&serde_json::json!({"id":plan.id,"token":plan.token,
            "version":plan.manifest.version,"sha256":plan.manifest.sha256,"outcome":"rolled_back"}))?,
        )?;
        launch(&plan, true)?;
        return Err(error);
    }
    Ok(())
}
/// These internal operations run before creating a runtime, tray or log writer.
pub fn early_operation() -> Option<Result<()>> {
    let mut args = std::env::args_os().skip(1);
    match args.next()?.to_str()? {
        "--dev-update-probe" => Some(if args.next().is_none() {
            println!(
                "{}",
                serde_json::json!({"dev_update":u8::from(cfg!(any(windows, target_os = "macos"))),"student":env!("CARGO_BIN_NAME") == "ClassroomStudent",
                "os":std::env::consts::OS,"arch":std::env::consts::ARCH,"build":env!("CLASSROOM_BUILD_ID")})
            );
            Ok(())
        } else {
            Err(anyhow::anyhow!("Invalid probe arguments"))
        }),
        "--dev-update-worker" => Some(
            args.next()
                .context("Missing update plan")
                .and_then(|p| worker(Path::new(&p))),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn supervisor_restores_and_relaunches_old_build_when_new_process_exits() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "classroom-rollback-test-{}",
            super::super::classroom_tools::nonce().unwrap()
        ));
        fs::create_dir(&dir).unwrap();
        let marker = dir.join("old-relaunched");
        let target = dir.join("student");
        let staged = dir.join("new");
        let old = format!("#!/bin/sh\nprintf 'restored' > '{}'\n", marker.display());
        fs::write(&target, &old).unwrap();
        fs::write(&staged, "#!/bin/sh\nexit 7\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755)).unwrap();
        let plan = Plan {
            id: "test".into(),
            token: "token".into(),
            manifest: Manifest {
                format: 1,
                os: "macos".into(),
                arch: "aarch64".into(),
                version: "new".into(),
                root: "student".into(),
                executable: "student".into(),
                sha256: hash(&staged).unwrap(),
            },
            target: target.clone(),
            staged,
            backup: dir.join("previous"),
            receipt: dir.join("receipt.json"),
            executable_suffix: PathBuf::new(),
            parent_pid: u32::MAX,
            args: vec![],
        };
        let path = dir.join("plan.json");
        fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
        assert!(worker(&path).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), old);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !marker.exists() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(&plan.receipt).unwrap()).unwrap();
        assert_eq!(receipt["outcome"], "rolled_back");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn extraction_rejects_traversal_and_symlinks() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!(
            "classroom-zip-test-{}",
            super::super::classroom_tools::nonce().unwrap()
        ));
        fs::create_dir(&dir).unwrap();
        let archive = dir.join("input.zip");
        for symlink in [false, true] {
            let mut writer = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
            if symlink {
                writer
                    .add_symlink(
                        "link",
                        "../escape",
                        zip::write::SimpleFileOptions::default(),
                    )
                    .unwrap();
            } else {
                writer
                    .start_file("../escape", zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(b"content").unwrap();
            }
            writer.finish().unwrap();
            let output = dir.join("output");
            fs::create_dir(&output).unwrap();
            assert!(extract(&archive, &output, &AtomicBool::new(false)).is_err());
            assert!(!dir.join("escape").exists());
            fs::remove_dir_all(output).unwrap();
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn package_paths_and_manifest_are_strict() {
        for bad in ["../app", "/tmp/app", "C:/app", "a\\b", ""] {
            assert!(safe_relative(bad).is_err(), "{bad}");
        }
        let mut m = Manifest {
            format: 1,
            os: "windows".into(),
            arch: "x86_64".into(),
            version: "test".into(),
            root: "ClassroomStudent.exe".into(),
            executable: "ClassroomStudent.exe".into(),
            sha256: "a".repeat(64),
        };
        assert!(m.validate().is_ok());
        m.executable = "outside.exe".into();
        assert!(m.validate().is_err());
        m.executable = m.root.clone();
        m.format = 99;
        assert!(m.validate().is_err());
    }
    #[test]
    fn swap_and_rollback_preserve_previous_build() {
        let dir = std::env::temp_dir().join(format!(
            "classroom-update-test-{}",
            super::super::classroom_tools::nonce().unwrap()
        ));
        fs::create_dir(&dir).unwrap();
        let target = dir.join("Student.exe");
        let staged = dir.join("new.exe");
        let backup = dir.join("old.exe");
        fs::write(&target, b"old").unwrap();
        fs::write(&staged, b"new").unwrap();
        let p = Plan {
            id: "test".into(),
            token: String::new(),
            manifest: Manifest {
                format: 1,
                os: "windows".into(),
                arch: "x86_64".into(),
                version: "test".into(),
                root: "Student.exe".into(),
                executable: "Student.exe".into(),
                sha256: "a".repeat(64),
            },
            target: target.clone(),
            staged,
            backup,
            receipt: dir.join("receipt"),
            executable_suffix: PathBuf::new(),
            parent_pid: 0,
            args: vec![],
        };
        swap(&p).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        rollback(&p).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"old");
        fs::remove_file(&p.staged).unwrap();
        assert!(swap(&p).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old");
        fs::remove_dir_all(dir).unwrap();
    }
}
