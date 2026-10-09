//! Network-level Internet control. Two cooperating parts, both system-wide so
//! no program (browser, native app, curl, custom code) can opt out:
//!
//! * a [`Firewall`] (Windows Filtering Platform on Windows) that blocks
//!   outbound connections to everything except the local machine, the
//!   classroom LAN and addresses learned for allowed names, and
//! * a local DNS proxy ([`dns::Proxy`], reached through a [`DnsRedirect`]) that
//!   turns site rules (wildcards included) into those addresses by watching
//!   which names get resolved, and refuses names that must not resolve.
//!
//! Loopback (`localhost`, `127.0.0.1:8080`…) and private networks are always
//! reachable so local web exercises and the Teacher keep working.
use super::{
    dns::{self, Decider, Decision},
    rules::{is_private, Cidr, RuleSet},
    Enforcer, Mode, Policy,
};
use anyhow::Result;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    thread,
    time::{Duration, Instant},
};
use tracing::{info, warn};

/// What the firewall must enforce for the current policy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Block every outbound connection that no permit below matches.
    pub default_block: bool,
    pub permit: Vec<Cidr>,
    /// Always blocked (unless permitted above): listed literal addresses and,
    /// for block lists, well-known DNS-over-HTTPS resolvers.
    pub block: Vec<Cidr>,
    pub endpoints: Vec<IpAddr>,
}
pub trait Firewall: Send {
    fn set_plan(&mut self, plan: &Plan) -> Result<()>;
    /// Addresses currently permitted because an allowed name resolved to them.
    fn set_learned(&mut self, ips: &[Ipv4Addr]) -> Result<()>;
    fn clear(&mut self);
}
/// Makes every DNS lookup on the machine go through our proxy.
pub trait DnsRedirect: Send {
    /// The resolvers the machine used before the redirect.
    fn upstream_servers(&self) -> Result<Vec<SocketAddr>>;
    fn install(&mut self) -> Result<()>;
    fn remove(&mut self);
    /// Forget cached answers so a changed policy takes effect immediately.
    fn flush(&mut self);
    fn copilot_cached_addresses(&self) -> Vec<IpAddr> {
        Vec::new()
    }
}

/// Public DNS-over-HTTPS/TLS resolvers a browser could use to skip our proxy.
const DOH_RESOLVERS: [&str; 14] = [
    "1.1.1.1",
    "1.0.0.1",
    "8.8.8.8",
    "8.8.4.4",
    "9.9.9.9",
    "149.112.112.112",
    "208.67.222.222",
    "208.67.220.220",
    "94.140.14.14",
    "94.140.15.15",
    "185.228.168.9",
    "185.228.169.9",
    "194.242.2.2",
    "45.90.28.1",
];
const LOCAL_NETS: [(&str, u8); 6] = [
    ("10.0.0.0", 8),
    ("172.16.0.0", 12),
    ("192.168.0.0", 16),
    ("169.254.0.0", 16),
    ("224.0.0.0", 4),
    ("127.0.0.0", 8),
];
/// Learned addresses stay usable this long beyond their DNS TTL.
const LEARN_GRACE: Duration = Duration::from_secs(60);
const MIN_TTL: u32 = 30;

struct Config {
    mode: Mode,
    rules: RuleSet,
    copilot: bool,
    plan: Plan,
    upstream: Vec<SocketAddr>,
}
struct Shared {
    config: RwLock<Config>,
    learned: Mutex<HashMap<Ipv4Addr, Instant>>,
    firewall: Mutex<Box<dyn Firewall>>,
}
impl Shared {
    fn push_learned(&self, learned: &HashMap<Ipv4Addr, Instant>) {
        let ips: Vec<Ipv4Addr> = learned.keys().copied().collect();
        if let Err(error) = self.firewall.lock().unwrap().set_learned(&ips) {
            warn!(%error, "Could not update permitted addresses");
        }
    }
}
fn host_is_local(name: &str) -> bool {
    name == "localhost" || name.ends_with(".localhost")
}
// Dedicated inference/proxy hosts only; do not block github.com or shared
// githubusercontent.com wholesale. Source: GitHub Copilot allowlist reference.
const COPILOT_DOMAINS: [&str; 3] = [
    "githubcopilot.com",
    "copilot-proxy.githubusercontent.com",
    "origin-tracker.githubusercontent.com",
];
const COPILOT_HOSTS: [&str; 9] = [
    "api.githubcopilot.com",
    "api.individual.githubcopilot.com",
    "api.business.githubcopilot.com",
    "api.enterprise.githubcopilot.com",
    "proxy.individual.githubcopilot.com",
    "proxy.business.githubcopilot.com",
    "proxy.enterprise.githubcopilot.com",
    "copilot-proxy.githubusercontent.com",
    "origin-tracker.githubusercontent.com",
];
fn copilot_host(name: &str) -> bool {
    COPILOT_DOMAINS
        .iter()
        .any(|d| name == *d || name.ends_with(&format!(".{d}")))
}
fn add_endpoint(plan: &mut Plan, ip: IpAddr) {
    match ip {
        IpAddr::V4(ip) if !is_private(ip) => {
            if !plan.endpoints.contains(&IpAddr::V4(ip)) {
                if plan.endpoints.len() >= 512 {
                    plan.endpoints.remove(0);
                }
                plan.endpoints.push(ip.into());
            }
        }
        IpAddr::V6(ip)
            if !ip.is_loopback()
                && !ip.is_unspecified()
                && (ip.segments()[0] & 0xfe00) != 0xfc00
                && (ip.segments()[0] & 0xffc0) != 0xfe80 =>
        {
            if !plan.endpoints.contains(&IpAddr::V6(ip)) {
                if plan.endpoints.len() >= 512 {
                    plan.endpoints.remove(0);
                }
                plan.endpoints.push(ip.into());
            }
        }
        _ => {}
    }
}
fn resolve_endpoints(upstream: &[SocketAddr], hosts: &[&str]) -> Vec<IpAddr> {
    // Fixed, bounded seed list; bypass our own NRPT redirect via the original
    // upstream sockets. No OS resolver call that could recurse into the proxy.
    std::thread::scope(|scope| {
        let workers: Vec<_> = hosts
            .chunks(3)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .flat_map(|host| dns::resolve_addresses(upstream, host))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap_or_default())
            .collect()
    })
}
impl Decider for Shared {
    fn decide(&self, name: &str, qtype: u16) -> Decision {
        let config = self.config.read().unwrap();
        // AAAA and HTTPS/SVCB records could smuggle addresses we never learn.
        let v6_or_hint = matches!(qtype, dns::TYPE_AAAA | 64 | 65);
        let passthrough = Decision::Forward {
            learn: false,
            private_only: false,
        };
        if host_is_local(name) {
            return passthrough;
        }
        if config.copilot && copilot_host(name) {
            return Decision::NxDomain;
        }
        match config.mode {
            Mode::Open => passthrough,
            Mode::BlockAll if v6_or_hint => Decision::Empty,
            Mode::BlockAll => Decision::Forward {
                learn: false,
                private_only: true,
            },
            Mode::AllowOnly if config.rules.matches_name(name) => {
                if v6_or_hint {
                    Decision::Empty
                } else {
                    Decision::Forward {
                        learn: qtype == dns::TYPE_A,
                        private_only: false,
                    }
                }
            }
            // Names that only point at the local network keep working.
            Mode::AllowOnly if v6_or_hint => Decision::Empty,
            Mode::AllowOnly => Decision::Forward {
                learn: false,
                private_only: true,
            },
            Mode::BlockOnly if config.rules.matches_name(name) => Decision::NxDomain,
            Mode::BlockOnly => passthrough,
        }
    }
    fn learned(&self, name: &str, ips: &[(Ipv4Addr, u32)]) {
        if self.config.read().unwrap().mode != Mode::AllowOnly {
            return;
        }
        let mut learned = self.learned.lock().unwrap();
        let mut added = false;
        for (ip, ttl) in ips.iter().filter(|(ip, _)| !is_private(*ip)) {
            let until =
                Instant::now() + Duration::from_secs(u64::from((*ttl).max(MIN_TTL))) + LEARN_GRACE;
            added |= learned.insert(*ip, until).is_none();
        }
        if added {
            info!(
                name,
                count = ips.len(),
                "Allowed name resolved; addresses permitted"
            );
            // Synchronous on purpose: the first connection follows the answer
            // by microseconds and must find the permit already in place.
            self.push_learned(&learned);
        }
    }
}

pub struct InternetEnforcer {
    shared: Arc<Shared>,
    redirect: Box<dyn DnsRedirect>,
    listen: SocketAddr,
    teacher_ip: Ipv4Addr,
    proxy: Option<dns::Proxy>,
    redirected: bool,
    upstream: Vec<SocketAddr>,
    stop: Arc<AtomicBool>,
    reaper: Option<thread::JoinHandle<()>>,
}
impl InternetEnforcer {
    pub fn new(
        firewall: Box<dyn Firewall>,
        redirect: Box<dyn DnsRedirect>,
        listen: SocketAddr,
        teacher_ip: Ipv4Addr,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                config: RwLock::new(Config {
                    mode: Mode::Open,
                    rules: RuleSet::default(),
                    copilot: false,
                    plan: Plan::default(),
                    upstream: Vec::new(),
                }),
                learned: Mutex::new(HashMap::new()),
                firewall: Mutex::new(firewall),
            }),
            redirect,
            listen,
            teacher_ip,
            proxy: None,
            redirected: false,
            upstream: Vec::new(),
            stop: Arc::new(AtomicBool::new(false)),
            reaper: None,
        }
    }
    fn plan(&self, policy: &Policy, rules: &RuleSet) -> Plan {
        let cidr = |ip: &str, prefix| Cidr {
            ip: ip.parse().unwrap_or(Ipv4Addr::UNSPECIFIED),
            prefix,
        };
        let mut permit: Vec<Cidr> = LOCAL_NETS.iter().map(|(ip, p)| cidr(ip, *p)).collect();
        permit.push(Cidr {
            ip: self.teacher_ip,
            prefix: 32,
        });
        permit.push(Cidr {
            ip: Ipv4Addr::BROADCAST,
            prefix: 32,
        });
        for server in &self.upstream {
            if let SocketAddr::V4(v4) = server {
                permit.push(Cidr {
                    ip: *v4.ip(),
                    prefix: 32,
                });
            }
        }
        let mut block = Vec::new();
        match policy.internet.mode {
            Mode::AllowOnly => permit.extend_from_slice(rules.nets()),
            Mode::BlockOnly => {
                block.extend_from_slice(rules.nets());
                block.extend(DOH_RESOLVERS.iter().map(|ip| cidr(ip, 32)));
            }
            _ => {}
        }
        if policy.copilot_block && policy.internet.mode != Mode::BlockOnly {
            block.extend(DOH_RESOLVERS.iter().map(|ip| cidr(ip, 32)));
        }
        Plan {
            default_block: matches!(policy.internet.mode, Mode::BlockAll | Mode::AllowOnly),
            permit,
            block,
            endpoints: Vec::new(),
        }
    }
    fn start_reaper(&mut self) {
        if self.reaper.is_some() {
            return;
        }
        let (shared, stop) = (Arc::clone(&self.shared), Arc::clone(&self.stop));
        self.reaper = Some(thread::spawn(move || {
            let mut refresh = Instant::now();
            while !stop.load(Ordering::Acquire) {
                for _ in 0..50 {
                    if stop.load(Ordering::Acquire) {
                        return;
                    }
                    thread::sleep(Duration::from_millis(200));
                }
                let mut learned = shared.learned.lock().unwrap();
                let before = learned.len();
                let now = Instant::now();
                learned.retain(|_, until| *until > now);
                if learned.len() != before {
                    shared.push_learned(&learned);
                }
                drop(learned);
                if refresh.elapsed() >= Duration::from_secs(60) {
                    refresh = Instant::now();
                    let upstream = {
                        let config = shared.config.read().unwrap();
                        if config.copilot {
                            config.upstream.clone()
                        } else {
                            Vec::new()
                        }
                    };
                    if !upstream.is_empty() {
                        let ips = resolve_endpoints(&upstream, &COPILOT_HOSTS);
                        let mut config = shared.config.write().unwrap();
                        if config.copilot {
                            // Retain earlier addresses for cached answers, evicting
                            // oldest entries at the fixed memory/filter bound.
                            for ip in ips {
                                add_endpoint(&mut config.plan, ip);
                            }
                            if let Err(error) =
                                shared.firewall.lock().unwrap().set_plan(&config.plan)
                            {
                                warn!(%error, "Could not refresh Copilot endpoint filters");
                            }
                        }
                    }
                }
            }
        }));
    }
}
impl Enforcer for InternetEnforcer {
    fn name(&self) -> &'static str {
        "internet"
    }
    fn apply(&mut self, policy: &Policy) -> Result<()> {
        let rules = RuleSet::parse(&policy.internet.rules)?;
        if self.upstream.is_empty() {
            self.upstream = self.redirect.upstream_servers()?;
        }
        if self.proxy.is_none() {
            self.proxy = Some(dns::Proxy::start(
                self.listen,
                self.upstream.clone(),
                Arc::clone(&self.shared) as Arc<dyn Decider>,
            )?);
        }
        let mut plan = self.plan(policy, &rules);
        if policy.copilot_block {
            let config = self.shared.config.read().unwrap();
            if config.copilot {
                for ip in &config.plan.endpoints {
                    add_endpoint(&mut plan, *ip);
                }
            }
            drop(config);
            for ip in self.redirect.copilot_cached_addresses() {
                add_endpoint(&mut plan, ip);
            }
            for ip in resolve_endpoints(&self.upstream, &COPILOT_HOSTS) {
                add_endpoint(&mut plan, ip);
            }
            // A zero-address result still has DNS blocking, but must never be
            // acknowledged as complete IP enforcement.
            anyhow::ensure!(
                !plan.endpoints.is_empty(),
                "Could not resolve any Copilot endpoint; retry when DNS is reachable"
            );
        }
        {
            let mut config = self.shared.config.write().unwrap();
            config.mode = policy.internet.mode;
            config.rules = rules;
            config.copilot = policy.copilot_block;
            config.plan = plan.clone();
            config.upstream = self.upstream.clone();
        }
        self.shared.learned.lock().unwrap().clear();
        {
            let mut firewall = self.shared.firewall.lock().unwrap();
            firewall.set_plan(&plan)?;
            firewall.set_learned(&[])?;
        }
        if !self.redirected {
            self.redirect.install()?;
            self.redirected = true;
        }
        self.redirect.flush();
        self.start_reaper();
        info!(
            mode = policy.internet.mode.name(),
            rules = policy.internet.rules.len(),
            "Internet policy enforced"
        );
        Ok(())
    }
    fn clear(&mut self) {
        {
            let mut config = self.shared.config.write().unwrap();
            config.mode = Mode::Open;
            config.copilot = false;
        }
        // Order matters: stop redirecting names first, then drop the filters.
        self.redirect.remove();
        self.redirected = false;
        self.shared.firewall.lock().unwrap().clear();
        self.shared.learned.lock().unwrap().clear();
        self.proxy = None;
        self.stop.store(true, Ordering::Release);
        if let Some(reaper) = self.reaper.take() {
            let _ = reaper.join();
        }
        self.stop = Arc::new(AtomicBool::new(false));
        self.upstream.clear();
    }
}
impl Drop for InternetEnforcer {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;

    #[derive(Default)]
    pub struct Calls {
        plan: Option<Plan>,
        learned: Vec<Ipv4Addr>,
        cleared: u32,
        redirects: u32,
        removed: u32,
        flushed: u32,
    }
    struct Fake(Arc<Mutex<Calls>>);
    impl Firewall for Fake {
        fn set_plan(&mut self, plan: &Plan) -> Result<()> {
            self.0.lock().unwrap().plan = Some(plan.clone());
            Ok(())
        }
        fn set_learned(&mut self, ips: &[Ipv4Addr]) -> Result<()> {
            self.0.lock().unwrap().learned = ips.to_vec();
            Ok(())
        }
        fn clear(&mut self) {
            self.0.lock().unwrap().cleared += 1;
        }
    }
    struct FakeRedirect(SocketAddr, Arc<Mutex<Calls>>);
    impl DnsRedirect for FakeRedirect {
        fn upstream_servers(&self) -> Result<Vec<SocketAddr>> {
            Ok(vec![self.0])
        }
        fn install(&mut self) -> Result<()> {
            self.1.lock().unwrap().redirects += 1;
            Ok(())
        }
        fn remove(&mut self) {
            self.1.lock().unwrap().removed += 1;
        }
        fn flush(&mut self) {
            self.1.lock().unwrap().flushed += 1;
        }
    }

    fn policy(mode: Mode, rules: &[&str]) -> Policy {
        Policy {
            internet: super::super::InternetPolicy {
                mode,
                rules: rules.iter().map(|s| (*s).into()).collect(),
            },
            lease_minutes: 60,
            copilot_block: false,
        }
    }
    fn setup() -> (InternetEnforcer, Arc<Mutex<Calls>>) {
        let upstream = UdpSocket::bind("127.0.0.1:0").unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        thread::spawn(move || {
            let mut buf = [0; 512];
            while let Ok((n, from)) = upstream.recv_from(&mut buf) {
                let ip = if buf[..n].windows(5).any(|w| w == b"local") {
                    "192.168.1.9"
                } else {
                    "93.184.216.34"
                };
                let _ = upstream.send_to(
                    &dns::build_answer(&buf[..n], &[(ip.parse().unwrap(), 120)]),
                    from,
                );
            }
        });
        let calls = Arc::new(Mutex::new(Calls::default()));
        let enforcer = InternetEnforcer::new(
            Box::new(Fake(calls.clone())),
            Box::new(FakeRedirect(upstream_addr, calls.clone())),
            "127.0.0.1:0".parse().unwrap(),
            "203.0.113.10".parse().unwrap(),
        );
        (enforcer, calls)
    }
    fn ask(enforcer: &InternetEnforcer, name: &str, qtype: u16) -> Vec<u8> {
        let addr = enforcer.proxy.as_ref().unwrap().udp_addr;
        let s = UdpSocket::bind("127.0.0.1:0").unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        s.send_to(&dns::build_query(5, name, qtype), addr).unwrap();
        let mut buf = [0; 512];
        let (n, _) = s.recv_from(&mut buf).unwrap();
        buf[..n].to_vec()
    }
    fn rcode(r: &[u8]) -> u8 {
        r[3] & 0x0f
    }

    #[test]
    fn block_all_keeps_only_local_and_lan_reachable() {
        let (mut e, calls) = setup();
        e.apply(&policy(Mode::BlockAll, &[])).unwrap();
        let plan = calls.lock().unwrap().plan.clone().unwrap();
        assert!(plan.default_block && plan.block.is_empty());
        for must in [
            "127.0.0.1",
            "10.1.1.1",
            "172.20.0.1",
            "192.168.5.5",
            "203.0.113.10",
        ] {
            assert!(
                plan.permit
                    .iter()
                    .any(|c| c.contains(must.parse().unwrap())),
                "{must}"
            );
        }
        assert!(!plan
            .permit
            .iter()
            .any(|c| c.contains("8.8.8.8".parse().unwrap())));
        assert_eq!(
            rcode(&ask(&e, "www.example.com", dns::TYPE_A)),
            3,
            "public names refused"
        );
        assert_eq!(
            dns::a_records(&ask(&e, "printer.local", dns::TYPE_A))[0].0,
            "192.168.1.9".parse::<Ipv4Addr>().unwrap()
        );
        assert_eq!(calls.lock().unwrap().redirects, 1);
        e.apply(&policy(Mode::BlockAll, &[])).unwrap();
        assert_eq!(
            calls.lock().unwrap().flushed,
            2,
            "every change flushes DNS caches"
        );
    }

    #[test]
    fn allow_only_learns_addresses_of_matching_names_only() {
        let (mut e, calls) = setup();
        e.apply(&policy(
            Mode::AllowOnly,
            &["*.wikipedia.org", "example.com", "198.51.100.0/24"],
        ))
        .unwrap();
        assert!(calls
            .lock()
            .unwrap()
            .plan
            .as_ref()
            .unwrap()
            .permit
            .iter()
            .any(|c| c.contains("198.51.100.7".parse().unwrap())));
        assert!(!ask(&e, "en.wikipedia.org", dns::TYPE_A).is_empty());
        assert_eq!(
            calls.lock().unwrap().learned,
            ["93.184.216.34".parse::<Ipv4Addr>().unwrap()]
        );
        assert_eq!(rcode(&ask(&e, "facebook.com", dns::TYPE_A)), 3);
        assert_eq!(
            rcode(&ask(&e, "www.example.com", dns::TYPE_AAAA)),
            0,
            "IPv6 answers are withheld"
        );
        assert!(dns::a_records(&ask(&e, "www.example.com", dns::TYPE_AAAA)).is_empty());
        // Re-applying resets what was learned.
        e.apply(&policy(Mode::AllowOnly, &["example.com"])).unwrap();
        assert!(calls.lock().unwrap().learned.is_empty());
    }

    #[test]
    fn block_only_sinkholes_listed_names_and_blocks_doh_but_not_the_upstream() {
        let (mut e, calls) = setup();
        e.apply(&policy(
            Mode::BlockOnly,
            &["facebook.com", "*.tiktok.com", "203.0.113.99"],
        ))
        .unwrap();
        let plan = calls.lock().unwrap().plan.clone().unwrap();
        assert!(!plan.default_block);
        assert!(plan
            .block
            .iter()
            .any(|c| c.contains("1.1.1.1".parse().unwrap())));
        assert!(plan
            .block
            .iter()
            .any(|c| c.contains("203.0.113.99".parse().unwrap())));
        assert_eq!(rcode(&ask(&e, "m.facebook.com", dns::TYPE_A)), 3);
        assert_eq!(rcode(&ask(&e, "v.tiktok.com", dns::TYPE_A)), 3);
        assert_eq!(rcode(&ask(&e, "wikipedia.org", dns::TYPE_A)), 0);
        assert_eq!(rcode(&ask(&e, "localhost", dns::TYPE_A)), 0);
    }

    #[test]
    fn clear_restores_everything_and_is_repeatable() {
        let (mut e, calls) = setup();
        e.apply(&policy(Mode::BlockAll, &[])).unwrap();
        e.clear();
        e.clear();
        let c = calls.lock().unwrap();
        assert!(c.removed >= 1 && c.cleared >= 1);
        assert!(e.proxy.is_none());
    }
}
