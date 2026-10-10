use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::Ipv4Addr,
    path::{Path, PathBuf},
};
use tokio_rustls::rustls::{
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
    ServerConfig,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub enabled: bool,
    #[serde(default)]
    pub allow_desktop_open: bool,
    pub teacher_ip: Ipv4Addr,
    pub teacher_certificate: String,
    pub secret: String,
}
pub fn policy_path() -> PathBuf {
    #[cfg(windows)]
    {
        super::windows_worker::policy_path()
            .unwrap_or_else(|_| PathBuf::from("C:/ProgramData/Classroom/remote-terminal.json"))
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/Library/Application Support/Classroom/remote-terminal.json")
    }
}
impl Policy {
    pub fn load() -> Result<Self> {
        let path = policy_path();
        #[cfg(windows)]
        {
            super::windows_worker::managed_acl(&path)?;
            super::windows_worker::managed_acl(path.parent().context("Policy directory missing")?)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let m = std::fs::metadata(&path)?;
            ensure!(
                m.uid() == 0 && m.mode() & 0o022 == 0,
                "Terminal policy must be managed by root"
            );
            let parent = std::fs::metadata(path.parent().context("Policy directory missing")?)?;
            ensure!(
                parent.uid() == 0 && parent.mode() & 0o022 == 0,
                "Terminal policy directory must be managed by root"
            );
        }
        ensure!(
            std::fs::metadata(&path)?.len() <= 8192,
            "Terminal policy exceeds limit"
        );
        let p: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        ensure!(
            p.enabled && STANDARD.decode(&p.secret)?.len() == 32,
            "Remote terminal is not authorized"
        );
        ensure!(!p.teacher_ip.is_unspecified(), "Invalid Teacher address");
        Ok(p)
    }
}
#[derive(Serialize, Deserialize)]
pub struct Identity {
    pub certificate: String,
    key: String,
    pub peers: HashMap<String, String>,
}
impl Identity {
    pub fn load(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let p = root.join("terminal-identity.json");
        if p.exists() {
            return Ok(serde_json::from_slice(&std::fs::read(p)?)?);
        }
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["classroom.local".into()])?;
        let identity = Self {
            certificate: STANDARD.encode(cert.der()),
            key: STANDARD.encode(signing_key.serialize_der()),
            peers: HashMap::new(),
        };
        identity.save(root)?;
        Ok(identity)
    }
    pub fn save(&self, root: &Path) -> Result<()> {
        private_write(
            &root.join("terminal-identity.json"),
            &serde_json::to_vec(self)?,
        )
    }
    pub fn server(&self) -> Result<ServerConfig> {
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(STANDARD.decode(&self.key)?));
        Ok(ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(STANDARD.decode(&self.certificate)?)],
                key,
            )?)
    }
}
pub fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let temporary = path.with_extension(format!("{}.tmp", crate::classroom_tools::nonce()?));
    let result = (|| -> Result<()> {
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
pub fn provision(root: &Path, student: &str, teacher_ip: Ipv4Addr) -> Result<serde_json::Value> {
    let mut identity = Identity::load(root)?;
    if !identity.peers.contains_key(student) {
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("OS random unavailable: {e}"))?;
        identity
            .peers
            .insert(student.into(), STANDARD.encode(bytes));
    }
    let secret = identity.peers[student].clone();
    identity.save(root)?;
    Ok(serde_json::to_value(Policy {
        enabled: true,
        allow_desktop_open: false,
        teacher_ip,
        teacher_certificate: identity.certificate,
        secret,
    })?)
}
pub fn audit(root: &Path, session: &str, event: &str, detail: serde_json::Value) -> Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(root)?;
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("remote-terminal-audit.lock"))?;
    // Serialize append and rotation across concurrent session worker processes.
    fs2::FileExt::lock_exclusive(&lock)?;
    let path = root.join("remote-terminal-audit.jsonl");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 5 * 1024 * 1024) {
        let previous = root.join("remote-terminal-audit.previous.jsonl");
        if previous.exists() {
            std::fs::remove_file(&previous)?;
        }
        std::fs::rename(&path, previous)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).context("Terminal audit unavailable")?;
    let event = serde_json::json!({"unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis(),"session":session,"event":event,"detail":detail});
    writeln!(file, "{event}")?;
    Ok(())
}
