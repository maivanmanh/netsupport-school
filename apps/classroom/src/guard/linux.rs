//! Linux Guard backends. Internet control needs root (nftables), but the Student
//! runs as the desktop user, so enforcement lives in a small root *helper*
//! (`ClassroomStudent --guard-helper`, a systemd service) and the Student talks
//! to it through [`RemoteEnforcer`] over a unix socket.
//!
//! * [`NftFirewall`]: an `inet classroom_guard` table in the output hook. Like the
//!   Windows WFP dynamic session it only exists while a Student is connected:
//!   the helper removes every rule when the controlling connection ends, so a
//!   crashed Student can never leave a computer cut off (the saved policy is
//!   re-applied when the Student starts again).
//! * [`NftRedirect`]: nat output rules send every DNS lookup of other users'
//!   processes to the helper's local proxy, which learns/permits addresses.
//! * The socket only accepts the one configured desktop uid (`SO_PEERCRED`).
#![allow(unsafe_code)]
use super::{
    internet::{DnsRedirect, Firewall, InternetEnforcer, Plan},
    rules::Cidr,
    Enforcer, Policy,
};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    os::unix::{
        fs::PermissionsExt,
        io::AsRawFd,
        net::{UnixListener, UnixStream},
    },
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use tracing::{info, warn};

pub const SOCKET_DIR: &str = "/run/classroom-guard";
pub const SOCKET: &str = "/run/classroom-guard/guard.sock";
const FILTER_TABLE: &str = "inet classroom_guard";
const NAT_TABLE: &str = "ip classroom_guard_nat";
/// Local DNS proxy port inside the helper.
pub const PROXY_PORT: u16 = 5354;

fn nft(script: &str) -> Result<()> {
    let mut child = Command::new("nft")
        .args(["-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("nftables (nft) is not installed")?;
    child
        .stdin
        .take()
        .context("nft stdin")?
        .write_all(script.as_bytes())?;
    let output = child.wait_with_output()?;
    anyhow::ensure!(
        output.status.success(),
        "nft failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

fn nets(list: &[Cidr]) -> String {
    list.iter()
        .map(|c| format!("{}/{}", c.ip, c.prefix))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The complete filter table for `plan` and the currently learned addresses.
pub fn filter_ruleset(plan: &Plan, learned: &[Ipv4Addr]) -> String {
    let mut v6_block = Vec::new();
    let mut v4_block: Vec<String> = plan.block.iter().map(|c| format!("{}/{}", c.ip, c.prefix)).collect();
    for ip in &plan.endpoints {
        match ip {
            IpAddr::V4(ip) => v4_block.push(ip.to_string()),
            IpAddr::V6(ip) => v6_block.push(ip.to_string()),
        }
    }
    let mut s = format!("add table {FILTER_TABLE}\ndelete table {FILTER_TABLE}\ntable {FILTER_TABLE} {{\n");
    let set = |name: &str, kind: &str, interval: bool, items: &str| {
        let mut text = format!("  set {name} {{\n    type {kind}\n");
        if interval {
            text.push_str("    flags interval\n");
        }
        if !items.is_empty() {
            text.push_str(&format!("    elements = {{ {items} }}\n"));
        }
        text.push_str("  }\n");
        text
    };
    s.push_str(&set("permit4", "ipv4_addr", true, &nets(&plan.permit)));
    s.push_str(&set("block4", "ipv4_addr", true, &v4_block.join(", ")));
    s.push_str(&set("block6", "ipv6_addr", false, &v6_block.join(", ")));
    s.push_str(&set(
        "learned4",
        "ipv4_addr",
        false,
        &learned.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "),
    ));
    s.push_str("  chain output {\n    type filter hook output priority 0; policy accept;\n");
    s.push_str("    oifname \"lo\" accept\n");
    s.push_str("    ip daddr @permit4 accept\n");
    s.push_str("    ip daddr @learned4 accept\n");
    s.push_str("    ip daddr @block4 ct state new reject\n");
    s.push_str("    ip6 daddr @block6 ct state new reject\n");
    if plan.default_block {
        s.push_str("    ip6 daddr { ::1, fe80::/10, fc00::/7, ff00::/8 } accept\n");
        s.push_str("    meta nfproto { ipv4, ipv6 } ct state new reject\n");
    }
    s.push_str("  }\n}\n");
    s
}

pub struct NftFirewall {
    plan: Plan,
    learned: Vec<Ipv4Addr>,
}
impl NftFirewall {
    pub fn new() -> Self {
        Self { plan: Plan::default(), learned: Vec::new() }
    }
    fn install(&self) -> Result<()> {
        nft(&filter_ruleset(&self.plan, &self.learned))
    }
}
impl Firewall for NftFirewall {
    fn set_plan(&mut self, plan: &Plan) -> Result<()> {
        self.plan = plan.clone();
        self.install()
    }
    fn set_learned(&mut self, ips: &[Ipv4Addr]) -> Result<()> {
        self.learned = ips.to_vec();
        // Only the learned set changes: no need to rebuild the whole table.
        let mut script = format!("flush set {FILTER_TABLE} learned4\n");
        if !ips.is_empty() {
            let list = ips.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
            script.push_str(&format!("add element {FILTER_TABLE} learned4 {{ {list} }}\n"));
        }
        nft(&script).or_else(|_| self.install())
    }
    fn clear(&mut self) {
        let _ = nft(&format!("add table {FILTER_TABLE}\ndelete table {FILTER_TABLE}\n"));
        self.plan = Plan::default();
        self.learned.clear();
    }
}

pub struct NftRedirect;
impl DnsRedirect for NftRedirect {
    fn upstream_servers(&self) -> Result<Vec<SocketAddr>> {
        let mut servers = Vec::new();
        for file in ["/run/systemd/resolve/resolv.conf", "/etc/resolv.conf"] {
            let Ok(text) = fs::read_to_string(file) else { continue };
            for line in text.lines() {
                let mut parts = line.split_whitespace();
                if parts.next() == Some("nameserver") {
                    if let Some(Ok(ip)) = parts.next().map(str::parse::<Ipv4Addr>) {
                        if !ip.is_loopback() && !ip.is_unspecified() {
                            servers.push(SocketAddr::new(ip.into(), 53));
                        }
                    }
                }
            }
            if !servers.is_empty() {
                break;
            }
        }
        anyhow::ensure!(!servers.is_empty(), "No DNS server is configured on this computer");
        Ok(servers)
    }
    fn install(&mut self) -> Result<()> {
        // Processes of root (the proxy itself, forwarding upstream) are exempt.
        nft(&format!(
            "add table {NAT_TABLE}\ndelete table {NAT_TABLE}\ntable {NAT_TABLE} {{\n  chain output {{\n    type nat hook output priority -100; policy accept;\n    meta skuid 0 return\n    udp dport 53 redirect to :{PROXY_PORT}\n    tcp dport 53 redirect to :{PROXY_PORT}\n  }}\n}}\n"
        ))
    }
    fn remove(&mut self) {
        let _ = nft(&format!("add table {NAT_TABLE}\ndelete table {NAT_TABLE}\n"));
    }
    fn flush(&mut self) {
        let _ = Command::new("resolvectl").arg("flush-caches").status();
    }
}

fn peer_uid(stream: &UnixStream) -> Option<u32> {
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `cred`/`len` are valid out-parameters for SO_PEERCRED on a live socket.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            std::ptr::addr_of_mut!(cred).cast(),
            &mut len,
        )
    };
    (rc == 0).then_some(cred.uid)
}

struct Active {
    id: u64,
    enforcer: InternetEnforcer,
}

fn reply(stream: &mut UnixStream, value: &Value) -> Result<()> {
    let mut line = value.to_string();
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    Ok(())
}

fn serve(mut stream: UnixStream, id: u64, active: &Arc<Mutex<Option<Active>>>) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    let mut hello = false;
    loop {
        line.clear();
        if reader.by_ref().take(256 * 1024).read_line(&mut line)? == 0 {
            break;
        }
        let request: Value = serde_json::from_str(line.trim()).context("invalid guard request")?;
        let outcome: Result<()> = (|| {
            match request["op"].as_str().unwrap_or("") {
                "ping" => Ok(()),
                "hello" => {
                    let teacher: Ipv4Addr = request["teacher_ip"].as_str().unwrap_or("").parse()?;
                    let mut slot = active.lock().unwrap();
                    // A new controller replaces the old one (its rules are cleared first).
                    *slot = Some(Active {
                        id,
                        enforcer: InternetEnforcer::new(
                            Box::new(NftFirewall::new()),
                            Box::new(NftRedirect),
                            SocketAddr::from(([127, 0, 0, 1], PROXY_PORT)),
                            teacher,
                        ),
                    });
                    hello = true;
                    Ok(())
                }
                "apply" => {
                    anyhow::ensure!(hello, "hello first");
                    let policy: Policy = serde_json::from_value(request["policy"].clone())?;
                    let mut slot = active.lock().unwrap();
                    let current = slot.as_mut().filter(|a| a.id == id).context("controller replaced")?;
                    current.enforcer.apply(&policy)
                }
                "clear" => {
                    if let Some(a) = active.lock().unwrap().as_mut().filter(|a| a.id == id) {
                        a.enforcer.clear();
                    }
                    Ok(())
                }
                other => anyhow::bail!("unknown op {other}"),
            }
        })();
        match outcome {
            Ok(()) => reply(&mut stream, &json!({"ok": true}))?,
            Err(error) => reply(&mut stream, &json!({"ok": false, "error": format!("{error:#}")}))?,
        }
    }
    // The controlling Student went away: remove everything it asked for.
    let mut slot = active.lock().unwrap();
    if slot.as_ref().is_some_and(|a| a.id == id) {
        *slot = None;
        info!("Guard controller disconnected; rules removed");
    }
    Ok(())
}

/// `ClassroomStudent --guard-helper --allow-uid N` (root, systemd service).
pub fn helper_main(allow_uid: u32) -> Result<()> {
    // SAFETY: plain query.
    anyhow::ensure!(unsafe { libc::geteuid() } == 0, "the Guard helper must run as root");
    fs::create_dir_all(SOCKET_DIR)?;
    let _ = fs::remove_file(SOCKET);
    let listener = UnixListener::bind(SOCKET).context("cannot bind the guard socket")?;
    fs::set_permissions(SOCKET, fs::Permissions::from_mode(0o666))?;
    // Start clean even after a crash.
    NftFirewall::new().clear();
    NftRedirect.remove();
    let active: Arc<Mutex<Option<Active>>> = Arc::new(Mutex::new(None));
    let mut next_id = 0_u64;
    info!(allow_uid, "Guard helper ready");
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        match peer_uid(&stream) {
            Some(uid) if uid == allow_uid || uid == 0 => {}
            other => {
                warn!(?other, "Guard connection from an unauthorized user refused");
                continue;
            }
        }
        next_id += 1;
        let (id, active) = (next_id, Arc::clone(&active));
        std::thread::spawn(move || {
            if let Err(error) = serve(stream, id, &active) {
                warn!(%error, "Guard connection ended with an error");
                let mut slot = active.lock().unwrap();
                if slot.as_ref().is_some_and(|a| a.id == id) {
                    *slot = None;
                }
            }
        });
    }
    Ok(())
}

/// Student-side enforcer that forwards the policy to the root helper.
pub struct RemoteEnforcer {
    teacher_ip: Ipv4Addr,
    conn: Option<(UnixStream, BufReader<UnixStream>)>,
}
impl RemoteEnforcer {
    pub fn new(teacher_ip: Ipv4Addr) -> Self {
        Self { teacher_ip, conn: None }
    }
    /// Whether the helper is installed and answering.
    pub fn available() -> bool {
        Path::new(SOCKET).exists() && UnixStream::connect(SOCKET).is_ok()
    }
    fn call(&mut self, request: &Value) -> Result<()> {
        if self.conn.is_none() {
            let stream = UnixStream::connect(SOCKET).context("Guard helper is not running")?;
            stream.set_read_timeout(Some(Duration::from_secs(20)))?;
            stream.set_write_timeout(Some(Duration::from_secs(5)))?;
            let reader = BufReader::new(stream.try_clone()?);
            self.conn = Some((stream, reader));
            self.exchange(&json!({"op": "hello", "teacher_ip": self.teacher_ip.to_string()}))?;
        }
        self.exchange(request)
    }
    fn exchange(&mut self, request: &Value) -> Result<()> {
        let (stream, reader) = self.conn.as_mut().context("no helper connection")?;
        let result = (|| -> Result<()> {
            let mut line = request.to_string();
            line.push('\n');
            stream.write_all(line.as_bytes())?;
            let mut answer = String::new();
            anyhow::ensure!(reader.read_line(&mut answer)? > 0, "Guard helper closed the connection");
            let value: Value = serde_json::from_str(answer.trim())?;
            anyhow::ensure!(value["ok"] == true, "{}", value["error"].as_str().unwrap_or("Guard helper refused"));
            Ok(())
        })();
        if result.is_err() {
            self.conn = None;
        }
        result
    }
}
impl Enforcer for RemoteEnforcer {
    fn name(&self) -> &'static str {
        "internet"
    }
    fn apply(&mut self, policy: &Policy) -> Result<()> {
        self.call(&json!({"op": "apply", "policy": policy}))
    }
    fn clear(&mut self) {
        if self.conn.is_some() || Self::available() {
            let _ = self.call(&json!({"op": "clear"}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::rules::Cidr;

    fn cidr(ip: &str, prefix: u8) -> Cidr {
        Cidr { ip: ip.parse().unwrap(), prefix }
    }

    #[test]
    fn allow_only_plan_becomes_a_default_reject_with_permits_first() {
        let plan = Plan {
            default_block: true,
            permit: vec![cidr("10.0.0.0", 8), cidr("93.184.216.34", 32)],
            block: vec![],
            endpoints: vec![],
        };
        let rules = filter_ruleset(&plan, &["1.2.3.4".parse().unwrap()]);
        let permit = rules.find("ip daddr @permit4 accept").unwrap();
        let block = rules.find("@block4 ct state new reject").unwrap();
        let default = rules.find("meta nfproto { ipv4, ipv6 } ct state new reject").unwrap();
        assert!(permit < block && block < default);
        assert!(rules.contains("elements = { 10.0.0.0/8, 93.184.216.34/32 }"));
        assert!(rules.contains("elements = { 1.2.3.4 }"));
        assert!(rules.contains("oifname \"lo\" accept"));
    }

    #[test]
    fn block_only_has_no_default_reject_and_lists_endpoints() {
        let plan = Plan {
            default_block: false,
            permit: vec![cidr("10.0.0.0", 8)],
            block: vec![cidr("198.51.100.0", 24)],
            endpoints: vec!["203.0.113.9".parse().unwrap(), "2001:db8::1".parse().unwrap()],
        };
        let rules = filter_ruleset(&plan, &[]);
        assert!(!rules.contains("meta nfproto"));
        assert!(rules.contains("198.51.100.0/24, 203.0.113.9"));
        assert!(rules.contains("2001:db8::1"));
    }
}
