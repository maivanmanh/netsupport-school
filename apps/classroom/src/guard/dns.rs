//! Minimal DNS wire handling and a local forwarding proxy. The proxy lets the
//! Guard decide per query name (allow / refuse) and learn which addresses an
//! allowed name resolves to *before* the answer reaches the application.
use anyhow::{Context, Result};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use tracing::{info, warn};

pub const TYPE_A: u16 = 1;
pub const TYPE_AAAA: u16 = 28;
const MAX_WORKERS: usize = 128;
const UPSTREAM_TIMEOUT: Duration = Duration::from_millis(2000);

#[derive(Debug, PartialEq, Eq)]
pub struct Query {
    pub name: String,
    pub qtype: u16,
    /// End of the question section (header + QNAME + QTYPE + QCLASS).
    pub question_end: usize,
}

fn read_u16(buf: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*buf.get(at)?, *buf.get(at + 1)?]))
}
fn read_u32(buf: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *buf.get(at)?,
        *buf.get(at + 1)?,
        *buf.get(at + 2)?,
        *buf.get(at + 3)?,
    ]))
}
/// Reads a (possibly compressed) name; returns it and the position after its
/// first occurrence.
fn read_name(buf: &[u8], start: usize) -> Option<(String, usize)> {
    let mut name = String::new();
    let (mut pos, mut end, mut jumps) = (start, None, 0);
    loop {
        let len = *buf.get(pos)?;
        if len & 0xc0 == 0xc0 {
            let target = usize::from(u16::from_be_bytes([len & 0x3f, *buf.get(pos + 1)?]));
            end.get_or_insert(pos + 2);
            jumps += 1;
            if jumps > 16 || target >= buf.len() {
                return None;
            }
            pos = target;
        } else if len == 0 {
            return Some((name, end.unwrap_or(pos + 1)));
        } else {
            let label = buf.get(pos + 1..pos + 1 + usize::from(len))?;
            if !name.is_empty() {
                name.push('.');
            }
            name.push_str(&String::from_utf8_lossy(label).to_ascii_lowercase());
            pos += 1 + usize::from(len);
            if name.len() > 255 {
                return None;
            }
        }
    }
}

/// A standard single-question query (what resolvers actually send).
pub fn parse_query(buf: &[u8]) -> Option<Query> {
    if buf.len() < 12 || buf[2] & 0x80 != 0 || read_u16(buf, 4)? != 1 {
        return None;
    }
    let (name, after) = read_name(buf, 12)?;
    let qtype = read_u16(buf, after)?;
    let question_end = after + 4;
    (question_end <= buf.len()).then_some(Query {
        name,
        qtype,
        question_end,
    })
}

fn reply(query: &[u8], rcode: u8) -> Option<Vec<u8>> {
    let q = parse_query(query)?;
    let mut out = Vec::with_capacity(q.question_end);
    out.extend_from_slice(&query[..2]);
    // QR, RD copied, RA set.
    out.push(0x80 | (query[2] & 0x01));
    out.push(0x80 | rcode);
    out.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&query[12..q.question_end]);
    Some(out)
}
/// "This name does not exist."
pub fn nxdomain(query: &[u8]) -> Option<Vec<u8>> {
    reply(query, 3)
}
/// "The name exists but has no records of this type" (forces IPv4 fallback).
pub fn empty(query: &[u8]) -> Option<Vec<u8>> {
    reply(query, 0)
}

/// `(address, ttl seconds)` of every A record in a response.
pub fn a_records(resp: &[u8]) -> Vec<(Ipv4Addr, u32)> {
    let mut found = Vec::new();
    let Some(count) = read_u16(resp, 6) else {
        return found;
    };
    let questions = read_u16(resp, 4).unwrap_or(0);
    let mut pos = 12;
    for _ in 0..questions {
        let Some((_, after)) = read_name(resp, pos) else {
            return found;
        };
        pos = after + 4;
    }
    for _ in 0..count {
        let Some((_, after)) = read_name(resp, pos) else {
            break;
        };
        let (Some(rtype), Some(ttl), Some(len)) = (
            read_u16(resp, after),
            read_u32(resp, after + 4),
            read_u16(resp, after + 8),
        ) else {
            break;
        };
        let data = after + 10;
        if rtype == TYPE_A && len == 4 {
            if let Some(b) = resp.get(data..data + 4) {
                found.push((Ipv4Addr::new(b[0], b[1], b[2], b[3]), ttl));
            }
        }
        pos = data + usize::from(len);
    }
    found
}

/// Resolve dedicated blocked endpoints through original upstream DNS, including
/// IPv6. Answers never pass through the local policy proxy.
pub fn resolve_addresses(upstream: &[SocketAddr], host: &str) -> Vec<std::net::IpAddr> {
    let mut ips: Vec<std::net::IpAddr> = Vec::new();
    for kind in [TYPE_A, TYPE_AAAA] {
        let mut query = vec![0x43, 0x50, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        for label in host.split('.') {
            query.push(label.len() as u8);
            query.extend_from_slice(label.as_bytes());
        }
        query.push(0);
        query.extend_from_slice(&kind.to_be_bytes());
        query.extend_from_slice(&[0, 1]);
        for server in upstream.iter().take(2) {
            let Ok(response) = forward_udp(*server, &query) else {
                continue;
            };
            if response.get(..2) != query.get(..2) {
                continue;
            }
            ips.extend(
                a_records(&response)
                    .into_iter()
                    .map(|(ip, _)| std::net::IpAddr::V4(ip)),
            );
            let mut pos = parse_query(&query).map_or(response.len(), |q| q.question_end);
            for _ in 0..read_u16(&response, 6).unwrap_or(0) {
                let Some((_, after)) = read_name(&response, pos) else {
                    break;
                };
                let Some(len) = read_u16(&response, after + 8) else {
                    break;
                };
                let data = after + 10;
                if read_u16(&response, after) == Some(TYPE_AAAA) && len == 16 {
                    if let Some(bytes) = response.get(data..data + 16) {
                        let mut octets = [0u8; 16];
                        octets.copy_from_slice(bytes);
                        ips.push(std::net::Ipv6Addr::from(octets).into());
                    }
                }
                pos = data + usize::from(len);
            }
            break;
        }
    }
    ips
}

pub enum Decision {
    /// Ask the real resolver. `private_only` turns answers that point at public
    /// addresses into NXDOMAIN; `learn` reports the addresses to the Guard.
    Forward {
        learn: bool,
        private_only: bool,
    },
    NxDomain,
    Empty,
}
pub trait Decider: Send + Sync {
    fn decide(&self, name: &str, qtype: u16) -> Decision;
    /// Called with the A records of an allowed answer before it is returned.
    fn learned(&self, name: &str, ips: &[(Ipv4Addr, u32)]);
}

pub struct Proxy {
    stop: Arc<AtomicBool>,
    threads: Vec<thread::JoinHandle<()>>,
    pub udp_addr: SocketAddr,
    pub tcp_addr: SocketAddr,
}
impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

struct Shared {
    upstream: Vec<SocketAddr>,
    decider: Arc<dyn Decider>,
    workers: AtomicUsize,
}
struct Worker<'a>(&'a AtomicUsize);
impl Drop for Worker<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Shared {
    fn answer(&self, query: &[u8], tcp: bool) -> Option<Vec<u8>> {
        let Some(q) = parse_query(query) else {
            return None;
        };
        match self.decider.decide(&q.name, q.qtype) {
            Decision::NxDomain => nxdomain(query),
            Decision::Empty => empty(query),
            Decision::Forward {
                learn,
                private_only,
            } => {
                let response = self.forward(query, tcp)?;
                let ips = a_records(&response);
                if private_only && ips.iter().any(|(ip, _)| !super::rules::is_private(*ip)) {
                    return nxdomain(query);
                }
                if learn && !ips.is_empty() {
                    self.decider.learned(&q.name, &ips);
                }
                Some(response)
            }
        }
    }
    fn forward(&self, query: &[u8], tcp: bool) -> Option<Vec<u8>> {
        for server in &self.upstream {
            let result = if tcp {
                forward_tcp(*server, query)
            } else {
                forward_udp(*server, query)
            };
            match result {
                Ok(response) => return Some(response),
                Err(error) => info!(%server, %error, "DNS upstream failed"),
            }
        }
        None
    }
}
fn forward_udp(server: SocketAddr, query: &[u8]) -> Result<Vec<u8>> {
    let socket = UdpSocket::bind(if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })?;
    socket.set_read_timeout(Some(UPSTREAM_TIMEOUT))?;
    socket.send_to(query, server)?;
    let mut buf = vec![0; 4096];
    loop {
        let (n, from) = socket.recv_from(&mut buf)?;
        if from == server && n >= 2 && buf[..2] == query[..2] {
            buf.truncate(n);
            return Ok(buf);
        }
    }
}
fn forward_tcp(server: SocketAddr, query: &[u8]) -> Result<Vec<u8>> {
    let mut s = TcpStream::connect_timeout(&server, UPSTREAM_TIMEOUT)?;
    s.set_read_timeout(Some(UPSTREAM_TIMEOUT * 2))?;
    s.set_write_timeout(Some(UPSTREAM_TIMEOUT))?;
    s.write_all(&u16::try_from(query.len())?.to_be_bytes())?;
    s.write_all(query)?;
    let mut len = [0; 2];
    s.read_exact(&mut len)?;
    let mut response = vec![0; usize::from(u16::from_be_bytes(len))];
    s.read_exact(&mut response)?;
    Ok(response)
}

impl Proxy {
    /// Listens on `listen` for UDP and TCP (TCP uses the same port when it is
    /// fixed, otherwise any free port, which tests rely on).
    pub fn start(
        listen: SocketAddr,
        upstream: Vec<SocketAddr>,
        decider: Arc<dyn Decider>,
    ) -> Result<Self> {
        anyhow::ensure!(!upstream.is_empty(), "No upstream DNS server known");
        let udp = UdpSocket::bind(listen).with_context(|| format!("Cannot bind UDP {listen}"))?;
        udp.set_read_timeout(Some(Duration::from_millis(200)))?;
        let udp_addr = udp.local_addr()?;
        let tcp_listen = if listen.port() == 0 {
            SocketAddr::new(listen.ip(), 0)
        } else {
            listen
        };
        let tcp = TcpListener::bind(tcp_listen)
            .with_context(|| format!("Cannot bind TCP {tcp_listen}"))?;
        tcp.set_nonblocking(true)?;
        let tcp_addr = tcp.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            upstream,
            decider,
            workers: AtomicUsize::new(0),
        });
        let mut threads = Vec::new();
        {
            let (stop, shared) = (Arc::clone(&stop), Arc::clone(&shared));
            let udp = Arc::new(udp);
            threads.push(thread::spawn(move || {
                let mut buf = vec![0; 4096];
                while !stop.load(Ordering::Acquire) {
                    let Ok((n, from)) = udp.recv_from(&mut buf) else {
                        continue;
                    };
                    if shared.workers.fetch_add(1, Ordering::AcqRel) >= MAX_WORKERS {
                        shared.workers.fetch_sub(1, Ordering::AcqRel);
                        continue;
                    }
                    let query = buf[..n].to_vec();
                    let (shared, udp) = (Arc::clone(&shared), Arc::clone(&udp));
                    thread::spawn(move || {
                        let _worker = Worker(&shared.workers);
                        if let Some(response) = shared.answer(&query, false) {
                            let _ = udp.send_to(&response, from);
                        }
                    });
                }
            }));
        }
        {
            let (stop, shared) = (Arc::clone(&stop), shared);
            threads.push(thread::spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    match tcp.accept() {
                        Ok((mut s, _)) => {
                            if shared.workers.fetch_add(1, Ordering::AcqRel) >= MAX_WORKERS {
                                shared.workers.fetch_sub(1, Ordering::AcqRel);
                                continue;
                            }
                            let shared = Arc::clone(&shared);
                            thread::spawn(move || {
                                let _worker = Worker(&shared.workers);
                                let _ = s.set_nonblocking(false);
                                let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
                                let mut len = [0; 2];
                                while s.read_exact(&mut len).is_ok() {
                                    let mut query = vec![0; usize::from(u16::from_be_bytes(len))];
                                    if s.read_exact(&mut query).is_err() {
                                        break;
                                    }
                                    let Some(response) = shared.answer(&query, true) else {
                                        break;
                                    };
                                    let Ok(n) = u16::try_from(response.len()) else {
                                        break;
                                    };
                                    if s.write_all(&n.to_be_bytes()).is_err()
                                        || s.write_all(&response).is_err()
                                    {
                                        break;
                                    }
                                }
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(20));
                        }
                        Err(error) => {
                            warn!(%error, "DNS TCP listener failed");
                            break;
                        }
                    }
                }
            }));
        }
        Ok(Self {
            stop,
            threads,
            udp_addr,
            tcp_addr,
        })
    }
}

#[cfg(test)]
pub(crate) fn build_query(id: u16, name: &str, qtype: u16) -> Vec<u8> {
    let mut q = vec![(id >> 8) as u8, id as u8, 0x01, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.split('.') {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.push(0);
    q.extend_from_slice(&qtype.to_be_bytes());
    q.extend_from_slice(&[0, 1]);
    q
}
#[cfg(test)]
pub(crate) fn build_answer(query: &[u8], ips: &[(Ipv4Addr, u32)]) -> Vec<u8> {
    let q = parse_query(query).unwrap();
    let mut r = query[..2].to_vec();
    r.extend_from_slice(&[0x81, 0x80, 0, 1]);
    r.extend_from_slice(&(ips.len() as u16).to_be_bytes());
    r.extend_from_slice(&[0, 0, 0, 0]);
    r.extend_from_slice(&query[12..q.question_end]);
    for (ip, ttl) in ips {
        r.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1]);
        r.extend_from_slice(&ttl.to_be_bytes());
        r.extend_from_slice(&[0, 4]);
        r.extend_from_slice(&ip.octets());
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn queries_answers_and_synthetic_replies_round_trip() {
        let query = build_query(0x1234, "WWW.Example.com", TYPE_A);
        let parsed = parse_query(&query).unwrap();
        assert_eq!(
            (parsed.name.as_str(), parsed.qtype),
            ("www.example.com", TYPE_A)
        );
        let nx = nxdomain(&query).unwrap();
        assert_eq!(
            (&nx[..2], nx[3] & 0x0f, nx[3] & 0x80),
            (&query[..2], 3, 0x80)
        );
        assert_eq!(empty(&query).unwrap()[3] & 0x0f, 0);
        let answer = build_answer(
            &query,
            &[
                ("93.184.216.34".parse().unwrap(), 300),
                ("10.0.0.1".parse().unwrap(), 5),
            ],
        );
        let found = a_records(&answer);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], ("93.184.216.34".parse().unwrap(), 300));
        assert!(parse_query(&answer).is_none(), "responses are not queries");
        assert!(parse_query(&[0; 5]).is_none());
    }

    struct Rules(Mutex<Vec<(String, Vec<(Ipv4Addr, u32)>)>>);
    impl Decider for Rules {
        fn decide(&self, name: &str, qtype: u16) -> Decision {
            if qtype == TYPE_AAAA {
                Decision::Empty
            } else if name.ends_with("allowed.test") {
                Decision::Forward {
                    learn: true,
                    private_only: false,
                }
            } else if name.ends_with("local.test") {
                Decision::Forward {
                    learn: false,
                    private_only: true,
                }
            } else {
                Decision::NxDomain
            }
        }
        fn learned(&self, name: &str, ips: &[(Ipv4Addr, u32)]) {
            self.0.lock().unwrap().push((name.to_owned(), ips.to_vec()));
        }
    }

    #[test]
    fn proxy_forwards_allowed_names_learns_their_addresses_and_refuses_the_rest() {
        let upstream = UdpSocket::bind("127.0.0.1:0").unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        thread::spawn(move || {
            let mut buf = [0; 512];
            while let Ok((n, from)) = upstream.recv_from(&mut buf) {
                let q = parse_query(&buf[..n]).unwrap();
                let ip = if q.name.starts_with("lan") {
                    "192.168.1.5"
                } else {
                    "93.184.216.34"
                };
                let _ = upstream.send_to(
                    &build_answer(&buf[..n], &[(ip.parse().unwrap(), 120)]),
                    from,
                );
            }
        });
        let decider = Arc::new(Rules(Mutex::new(Vec::new())));
        let proxy = Proxy::start(
            "127.0.0.1:0".parse().unwrap(),
            vec![upstream_addr],
            decider.clone(),
        )
        .unwrap();
        let ask = |name: &str, qtype: u16| {
            let s = UdpSocket::bind("127.0.0.1:0").unwrap();
            s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let q = build_query(7, name, qtype);
            s.send_to(&q, proxy.udp_addr).unwrap();
            let mut buf = [0; 512];
            let (n, _) = s.recv_from(&mut buf).unwrap();
            buf[..n].to_vec()
        };
        let ok = ask("www.allowed.test", TYPE_A);
        assert_eq!(
            a_records(&ok)[0].0,
            "93.184.216.34".parse::<Ipv4Addr>().unwrap()
        );
        assert_eq!(decider.0.lock().unwrap()[0].0, "www.allowed.test");
        assert_eq!(ask("blocked.example", TYPE_A)[3] & 0x0f, 3, "NXDOMAIN");
        assert_eq!(ask("www.allowed.test", TYPE_AAAA)[3] & 0x0f, 0, "no AAAA");
        assert_eq!(
            a_records(&ask("lan.local.test", TYPE_A))[0].0,
            "192.168.1.5".parse::<Ipv4Addr>().unwrap()
        );
        assert_eq!(
            ask("wan.local.test", TYPE_A)[3] & 0x0f,
            3,
            "public answer refused"
        );
        // The same decision applies over TCP.
        let mut tcp = TcpStream::connect(proxy.tcp_addr).unwrap();
        let q = build_query(9, "nope.example", TYPE_A);
        tcp.write_all(&(q.len() as u16).to_be_bytes()).unwrap();
        tcp.write_all(&q).unwrap();
        let mut len = [0; 2];
        tcp.read_exact(&mut len).unwrap();
        let mut resp = vec![0; usize::from(u16::from_be_bytes(len))];
        tcp.read_exact(&mut resp).unwrap();
        assert_eq!(resp[3] & 0x0f, 3);
    }
}
