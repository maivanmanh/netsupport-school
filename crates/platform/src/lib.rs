use std::net::Ipv4Addr;

use netdev::get_interfaces;
use thiserror::Error;

pub const AUTO_INTERFACE: &str = "auto_default_route_ethernet";

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct AdapterCandidate {
    pub index: u32,
    pub name: String,
    pub friendly_name: Option<String>,
    pub description: Option<String>,
    pub kind: String,
    pub transmit_speed_bps: Option<u64>,
    pub receive_speed_bps: Option<u64>,
    pub mtu: Option<u32>,
    pub ipv4: Vec<Ipv4Addr>,
    pub is_up: bool,
    pub is_default_route: bool,
    pub is_physical: bool,
    pub has_gateway: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedAdapter {
    pub candidate: AdapterCandidate,
    pub address: Ipv4Addr,
    pub score: i32,
    pub reason: String,
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("no active IPv4 network adapter matched selector '{0}'")]
    NoMatch(String),
}

#[must_use]
pub fn discover_adapters() -> Vec<AdapterCandidate> {
    // get_default_interface() enumerates all interfaces internally. Repeating
    // discovery can duplicate expensive OS configuration queries at startup.
    let interfaces = get_interfaces();
    let default_index = interfaces
        .iter()
        .find(|interface| interface.default)
        .map(|interface| interface.index)
        .or_else(|| {
            let local_ip = netdev::net::ip::get_local_ipaddr()?;
            interfaces
                .iter()
                .find(|interface| match local_ip {
                    std::net::IpAddr::V4(ip) => interface.ipv4.iter().any(|net| net.addr() == ip),
                    std::net::IpAddr::V6(ip) => interface.ipv6.iter().any(|net| net.addr() == ip),
                })
                .map(|interface| interface.index)
        });
    interfaces
        .into_iter()
        .map(|interface| AdapterCandidate {
            index: interface.index,
            name: interface.name.clone(),
            friendly_name: interface.friendly_name.clone(),
            description: interface.description.clone(),
            kind: format!("{:?}", interface.if_type),
            transmit_speed_bps: interface.transmit_speed,
            receive_speed_bps: interface.receive_speed,
            mtu: interface.mtu,
            ipv4: interface
                .ipv4
                .iter()
                .map(netdev::prelude::Ipv4Net::addr)
                .collect(),
            is_up: interface.is_up() && interface.is_oper_up(),
            is_default_route: interface.default || default_index == Some(interface.index),
            is_physical: interface.is_physical(),
            has_gateway: interface.gateway.is_some(),
        })
        .collect()
}

/// Selects an active adapter from the operating-system snapshot.
///
/// # Errors
/// Returns an error when no active, usable IPv4 adapter matches the selector.
pub fn select_adapter(selector: &str) -> Result<SelectedAdapter, AdapterError> {
    select_adapter_from(&discover_adapters(), selector)
}

/// Selects an adapter from a supplied snapshot. This is public for deterministic diagnostics.
///
/// # Errors
/// Returns an error when no active, usable IPv4 adapter matches the selector.
pub fn select_adapter_from(
    candidates: &[AdapterCandidate],
    selector: &str,
) -> Result<SelectedAdapter, AdapterError> {
    let explicit =
        !selector.eq_ignore_ascii_case(AUTO_INTERFACE) && !selector.eq_ignore_ascii_case("auto");
    candidates
        .iter()
        .filter(|candidate| candidate.is_up)
        .filter_map(|candidate| {
            if explicit && !matches_selector(candidate, selector) {
                return None;
            }
            let address = select_address(candidate, selector, explicit)?;
            let score = if explicit { i32::MAX } else { rank(candidate) };
            Some(SelectedAdapter {
                candidate: candidate.clone(),
                address,
                score,
                reason: selection_reason(candidate, explicit),
            })
        })
        .max_by_key(|selected| selected.score)
        .ok_or_else(|| AdapterError::NoMatch(selector.to_owned()))
}

#[must_use]
pub fn infer_teacher_address(local: Ipv4Addr, fallback: Ipv4Addr, mode: &str) -> Ipv4Addr {
    if mode == "explicit" {
        return fallback;
    }
    let octets = local.octets();
    if octets[0] == 10 && octets[1] == 0 {
        Ipv4Addr::new(10, 0, octets[2], 200)
    } else {
        fallback
    }
}

fn rank(candidate: &AdapterCandidate) -> i32 {
    let mut score = 0;
    if candidate.is_default_route {
        score += 1_000;
    }
    if candidate.is_physical {
        score += 500;
    }
    if candidate.kind.to_ascii_lowercase().contains("ethernet") {
        score += 350;
    }
    if candidate.has_gateway {
        score += 150;
    }
    if looks_virtual(candidate) {
        score -= 2_000;
    }
    if candidate.kind.to_ascii_lowercase().contains("wireless") {
        score -= 200;
    }
    score
}

fn matches_selector(candidate: &AdapterCandidate, selector: &str) -> bool {
    let selector = selector.to_ascii_lowercase();
    candidate
        .ipv4
        .iter()
        .any(|address| address.to_string() == selector)
        || candidate.name.to_ascii_lowercase() == selector
        || candidate
            .friendly_name
            .as_deref()
            .is_some_and(|value| value.to_ascii_lowercase() == selector)
        || candidate
            .description
            .as_deref()
            .is_some_and(|value| value.to_ascii_lowercase() == selector)
}

fn select_address(
    candidate: &AdapterCandidate,
    selector: &str,
    explicit: bool,
) -> Option<Ipv4Addr> {
    if explicit {
        if let Ok(requested) = selector.parse::<Ipv4Addr>() {
            if candidate.ipv4.contains(&requested) && is_usable_ipv4(requested) {
                return Some(requested);
            }
        }
    }
    candidate
        .ipv4
        .iter()
        .copied()
        .find(|address| is_usable_ipv4(*address))
}

fn looks_virtual(candidate: &AdapterCandidate) -> bool {
    let text = format!(
        "{} {} {} {}",
        candidate.name,
        candidate.friendly_name.as_deref().unwrap_or_default(),
        candidate.description.as_deref().unwrap_or_default(),
        candidate.kind
    )
    .to_ascii_lowercase();
    [
        "hyper-v",
        "vmware",
        "virtual",
        "vethernet",
        "vmnet",
        "loopback",
        "tunnel",
        "vpn",
        "tap",
        "tun",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn is_usable_ipv4(address: Ipv4Addr) -> bool {
    !address.is_loopback()
        && !address.is_link_local()
        && !address.is_unspecified()
        && !address.is_broadcast()
}

fn selection_reason(candidate: &AdapterCandidate, explicit: bool) -> String {
    if explicit {
        "manual interface override".to_owned()
    } else {
        format!(
            "automatic ranking: default_route={}, physical={}, ethernet={}, gateway={}, virtual={}",
            candidate.is_default_route,
            candidate.is_physical,
            candidate.kind.to_ascii_lowercase().contains("ethernet"),
            candidate.has_gateway,
            looks_virtual(candidate)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(
        name: &str,
        ip: [u8; 4],
        default: bool,
        physical: bool,
        kind: &str,
    ) -> AdapterCandidate {
        AdapterCandidate {
            index: 1,
            name: name.to_owned(),
            friendly_name: Some(name.to_owned()),
            description: None,
            kind: kind.to_owned(),
            transmit_speed_bps: None,
            receive_speed_bps: None,
            mtu: None,
            ipv4: vec![Ipv4Addr::from(ip)],
            is_up: true,
            is_default_route: default,
            is_physical: physical,
            has_gateway: default,
        }
    }

    #[test]
    fn physical_ethernet_beats_default_virtual_adapter() {
        let candidates = vec![
            adapter("VMware VMnet8", [192, 168, 44, 1], true, false, "Ethernet"),
            adapter(
                "Intel Ethernet",
                [10, 0, 55, 21],
                false,
                true,
                "GigabitEthernet",
            ),
        ];
        assert_eq!(
            select_adapter_from(&candidates, AUTO_INTERFACE)
                .unwrap()
                .address,
            Ipv4Addr::new(10, 0, 55, 21)
        );
    }

    #[test]
    fn default_route_breaks_tie_between_physical_ethernet_adapters() {
        let candidates = vec![
            adapter("Ethernet 2", [10, 0, 63, 20], false, true, "Ethernet"),
            adapter("Ethernet", [10, 0, 55, 20], true, true, "Ethernet"),
        ];
        assert_eq!(
            select_adapter_from(&candidates, AUTO_INTERFACE)
                .unwrap()
                .candidate
                .name,
            "Ethernet"
        );
    }

    #[test]
    fn manual_override_can_select_virtual_adapter() {
        let candidates = vec![adapter(
            "VMware VMnet8",
            [192, 168, 44, 1],
            true,
            false,
            "Ethernet",
        )];
        assert_eq!(
            select_adapter_from(&candidates, "VMware VMnet8")
                .unwrap()
                .address,
            Ipv4Addr::new(192, 168, 44, 1)
        );
    }

    #[test]
    fn manual_ip_override_can_select_a_secondary_address() {
        let mut candidate = adapter("Intel Ethernet", [10, 0, 55, 21], true, true, "Ethernet");
        candidate.ipv4.push(Ipv4Addr::new(10, 0, 55, 22));
        assert_eq!(
            select_adapter_from(&[candidate], "10.0.55.22")
                .unwrap()
                .address,
            Ipv4Addr::new(10, 0, 55, 22)
        );
    }

    #[test]
    fn teacher_address_uses_room_octet_only_for_campus_shape() {
        assert_eq!(
            infer_teacher_address(
                Ipv4Addr::new(10, 0, 63, 11),
                Ipv4Addr::new(10, 0, 55, 200),
                "infer_room_then_fallback"
            ),
            Ipv4Addr::new(10, 0, 63, 200)
        );
        assert_eq!(
            infer_teacher_address(
                Ipv4Addr::new(192, 168, 1, 11),
                Ipv4Addr::new(10, 0, 55, 200),
                "infer_room_then_fallback"
            ),
            Ipv4Addr::new(10, 0, 55, 200)
        );
    }
}
