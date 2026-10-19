//! Site rules typed by the Teacher: host names, `*` wildcards and IPv4
//! addresses/CIDR ranges, one per line. Ports and paths are not enforceable at
//! network level (HTTPS hides them), so they are stripped.
use anyhow::Result;
use std::net::Ipv4Addr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pattern {
    /// `example.com`: that host and every sub-domain.
    Domain(String),
    /// Contains `*`; matches the whole host name.
    Glob(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cidr {
    pub ip: Ipv4Addr,
    pub prefix: u8,
}
impl Cidr {
    pub fn mask(self) -> u32 {
        if self.prefix == 0 {
            0
        } else {
            u32::MAX << (32 - u32::from(self.prefix))
        }
    }
    pub fn contains(self, ip: Ipv4Addr) -> bool {
        u32::from(ip) & self.mask() == u32::from(self.ip) & self.mask()
    }
}

#[derive(Clone, Debug, Default)]
pub struct RuleSet {
    names: Vec<Pattern>,
    nets: Vec<Cidr>,
}
impl RuleSet {
    pub fn parse(lines: &[String]) -> Result<Self> {
        let mut set = Self::default();
        let mut problems = Vec::new();
        for line in lines {
            match parse_line(line) {
                Ok(Some(Entry::Name(p))) => set.names.push(p),
                Ok(Some(Entry::Net(n))) => set.nets.push(n),
                Ok(None) => {}
                Err(e) => problems.push(format!("“{}”: {e}", line.trim())),
            }
        }
        anyhow::ensure!(problems.is_empty(), "{}", problems.join("; "));
        Ok(set)
    }
    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.nets.is_empty()
    }
    pub fn nets(&self) -> &[Cidr] {
        &self.nets
    }
    pub fn matches_name(&self, host: &str) -> bool {
        let host = normalize_host(host);
        self.names.iter().any(|p| match p {
            Pattern::Domain(d) => host == *d || host.ends_with(&format!(".{d}")),
            Pattern::Glob(g) => glob(g.as_bytes(), host.as_bytes()),
        })
    }
    pub fn matches_ip(&self, ip: Ipv4Addr) -> bool {
        self.nets.iter().any(|n| n.contains(ip))
    }
}

pub fn normalize_host(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

pub enum Entry {
    Name(Pattern),
    Net(Cidr),
}
/// `Ok(None)` for blank lines and `#` comments.
pub fn parse_line(line: &str) -> Result<Option<Entry>> {
    let mut text = line.trim();
    if text.is_empty() || text.starts_with('#') {
        return Ok(None);
    }
    if let Some((scheme, rest)) = text.split_once("://") {
        anyhow::ensure!(
            scheme.chars().all(|c| c.is_ascii_alphabetic()),
            "unsupported address"
        );
        text = rest;
    }
    text = text.rsplit_once('@').map_or(text, |(_, host)| host);
    // CIDR keeps its '/', everything else loses the path.
    if let Some((ip, prefix)) = text.split_once('/') {
        if let (Ok(ip), Ok(prefix)) = (ip.parse::<Ipv4Addr>(), prefix.parse::<u8>()) {
            anyhow::ensure!(prefix <= 32, "prefix must be 0–32");
            return Ok(Some(Entry::Net(Cidr { ip, prefix })));
        }
    }
    let end = text.find(['/', '?', '#']).unwrap_or(text.len());
    let mut host = &text[..end];
    if let Some((h, port)) = host.rsplit_once(':') {
        if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) {
            host = h;
        }
    }
    let host = normalize_host(host);
    anyhow::ensure!(!host.is_empty(), "empty name");
    anyhow::ensure!(host.len() <= 253, "name too long");
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Ok(Some(Entry::Net(Cidr { ip, prefix: 32 })));
    }
    anyhow::ensure!(
        host.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'*')),
        "only letters, digits, '.', '-' and the * wildcard are allowed"
    );
    let host = host.trim_start_matches('.').to_owned();
    Ok(Some(Entry::Name(if host.contains('*') {
        Pattern::Glob(host)
    } else {
        Pattern::Domain(host)
    })))
}

fn glob(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            mark = t;
            p += 1;
        } else if let Some(s) = star {
            p = s + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&b| b == b'*')
}

pub fn is_private(ip: Ipv4Addr) -> bool {
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_unspecified()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(lines: &[&str]) -> RuleSet {
        RuleSet::parse(&lines.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn plain_domains_cover_subdomains_but_not_lookalikes() {
        let rules = set(&["facebook.com"]);
        assert!(rules.matches_name("facebook.com"));
        assert!(rules.matches_name("WWW.Facebook.com."));
        assert!(rules.matches_name("a.b.facebook.com"));
        assert!(!rules.matches_name("notfacebook.com"));
        assert!(!rules.matches_name("facebook.com.evil.org"));
    }

    #[test]
    fn wildcards_match_whole_names() {
        let rules = set(&["*.google.com", "mail.*.edu.vn", "cdn*.example.org"]);
        assert!(rules.matches_name("docs.google.com"));
        assert!(rules.matches_name("a.b.google.com"));
        assert!(!rules.matches_name("google.com"));
        assert!(rules.matches_name("mail.hcm.edu.vn"));
        assert!(rules.matches_name("cdn1.example.org"));
        assert!(rules.matches_name("cdn-eu-12.example.org"));
        assert!(!rules.matches_name("xcdn1.example.org"));
    }

    #[test]
    fn urls_are_reduced_to_hosts_and_networks_are_kept() {
        let rules = set(&[
            "# comment",
            "",
            "https://user@Example.com:8443/a/b?c=d",
            "10.1.2.3",
            "203.0.113.0/24",
        ]);
        assert!(rules.matches_name("example.com"));
        assert!(rules.matches_ip("10.1.2.3".parse().unwrap()));
        assert!(rules.matches_ip("203.0.113.77".parse().unwrap()));
        assert!(!rules.matches_ip("203.0.114.1".parse().unwrap()));
    }

    #[test]
    fn invalid_lines_are_reported() {
        let error = RuleSet::parse(&["good.com".into(), "bad host".into(), "1.2.3.4/40".into()])
            .unwrap_err()
            .to_string();
        assert!(error.contains("bad host") && error.contains("1.2.3.4/40"));
    }

    #[test]
    fn private_ranges() {
        for ip in [
            "10.0.0.5",
            "172.16.49.233",
            "192.168.1.1",
            "127.0.0.1",
            "169.254.1.1",
            "224.0.0.251",
        ] {
            assert!(is_private(ip.parse().unwrap()), "{ip}");
        }
        assert!(!is_private("8.8.8.8".parse().unwrap()));
    }
}
