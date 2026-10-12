use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{net::Ipv4Addr, path::Path};
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
impl Policy {
    /// Session authorization arrives only over the Student's configured Teacher
    /// tools connection. It is not a persistent administrator pairing policy.
    pub fn validate(&self, teacher_ip: &Ipv4Addr) -> Result<()> {
        ensure!(self.enabled, "Remote terminal session is disabled");
        ensure!(
            !teacher_ip.is_unspecified() && self.teacher_ip == *teacher_ip,
            "Terminal authorization must match configured Teacher"
        );
        ensure!(
            STANDARD.decode(&self.secret)?.len() == 32,
            "Invalid terminal session secret"
        );
        ensure!(
            self.teacher_certificate.len() <= 4096
                && !STANDARD.decode(&self.teacher_certificate)?.is_empty(),
            "Invalid terminal Teacher certificate"
        );
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
pub struct Identity {
    pub certificate: String,
    key: String,
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
