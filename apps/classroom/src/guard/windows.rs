//! Windows backends for the Guard: Windows Filtering Platform for the firewall
//! and a Name Resolution Policy Table rule for the DNS redirect. Requires an
//! elevated (administrator) process; [`is_elevated`] tells the Student up front.
//!
//! WFP filters live in a *dynamic* session: Windows removes them by itself if
//! the process ends for any reason, so a crashed Student can never leave the
//! machine cut off from the network (the persisted policy is re-applied on the
//! next start instead). The NRPT rule is the one piece that outlives a crash;
//! `Guard::restore` removes it at start-up and `scripts/windows-guard-reset.ps1`
//! does so by hand.
#![allow(unsafe_code)]
use super::internet::{DnsRedirect, Firewall, Plan};
use super::rules::Cidr;
use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr},
    os::windows::process::CommandExt,
    process::Command,
};
use windows::{
    core::{GUID, PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        NetworkManagement::WindowsFilteringPlatform::{
            FwpmEngineClose0, FwpmEngineOpen0, FwpmFilterAdd0, FwpmFilterDeleteById0,
            FwpmSubLayerAdd0, FwpmTransactionAbort0, FwpmTransactionBegin0, FwpmTransactionCommit0,
            FWPM_ACTION0, FWPM_CONDITION_IP_REMOTE_ADDRESS, FWPM_DISPLAY_DATA0, FWPM_FILTER0,
            FWPM_FILTER_CONDITION0, FWPM_LAYER_ALE_AUTH_CONNECT_V4, FWPM_LAYER_ALE_AUTH_CONNECT_V6,
            FWPM_SESSION0, FWPM_SESSION_FLAG_DYNAMIC, FWPM_SUBLAYER0, FWP_ACTION_BLOCK,
            FWP_ACTION_PERMIT, FWP_ACTION_TYPE, FWP_MATCH_EQUAL, FWP_UINT8, FWP_V4_ADDR_AND_MASK,
            FWP_V4_ADDR_MASK, FWP_V6_ADDR_AND_MASK, FWP_V6_ADDR_MASK, FWP_VALUE0, FWP_VALUE0_0,
        },
        Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
        System::{
            Rpc::RPC_C_AUTHN_DEFAULT,
            Threading::{GetCurrentProcess, OpenProcessToken},
        },
    },
};

const NO_WINDOW: u32 = 0x0800_0000;
/// Fixed so a restarted Student never accumulates sub-layers.
const SUBLAYER: GUID = GUID::from_u128(0x7c1a5e0d_3b9f_4c21_9a55_c1a5500f0001);

pub fn is_elevated() -> bool {
    // SAFETY: standard token query on the current process; the handle is closed.
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(std::ptr::addr_of_mut!(elevation).cast()),
            u32::try_from(std::mem::size_of::<TOKEN_ELEVATION>()).unwrap_or(0),
            &mut returned,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

/// Weights inside our sub-layer (higher is evaluated first).
mod weight {
    pub const ENDPOINT: u8 = 15;
    pub const ESSENTIAL: u8 = 14; // Teacher, upstream DNS
    pub const BLOCK_LISTED: u8 = 13;
    pub const PERMIT: u8 = 12; // local networks, allowed names
    pub const DEFAULT_BLOCK: u8 = 1;
}

struct Engine(HANDLE);
// SAFETY: a WFP engine handle is an RPC binding that may be used from any thread.
unsafe impl Send for Engine {}
impl Drop for Engine {
    fn drop(&mut self) {
        // SAFETY: the handle came from FwpmEngineOpen0; closing also removes the
        // dynamic session's filters.
        unsafe {
            let _ = FwpmEngineClose0(self.0);
        }
    }
}

pub struct WfpFirewall {
    engine: Option<Engine>,
    plan_ids: Vec<u64>,
    learned: HashMap<Ipv4Addr, Vec<u64>>,
}
impl WfpFirewall {
    pub fn new() -> Self {
        Self {
            engine: None,
            plan_ids: Vec::new(),
            learned: HashMap::new(),
        }
    }
    fn engine(&mut self) -> Result<HANDLE> {
        if let Some(engine) = &self.engine {
            return Ok(engine.0);
        }
        let mut name: Vec<u16> = "Classroom Guard".encode_utf16().chain(Some(0)).collect();
        let session = FWPM_SESSION0 {
            flags: FWPM_SESSION_FLAG_DYNAMIC,
            displayData: FWPM_DISPLAY_DATA0 {
                name: PWSTR(name.as_mut_ptr()),
                description: PWSTR::null(),
            },
            ..Default::default()
        };
        let mut handle = HANDLE::default();
        // SAFETY: all pointers are valid for the duration of the call.
        let status = unsafe {
            FwpmEngineOpen0(
                PCWSTR::null(),
                RPC_C_AUTHN_DEFAULT as u32,
                None,
                Some(&session),
                &mut handle,
            )
        };
        anyhow::ensure!(
            status == 0,
            "FwpmEngineOpen0 failed (0x{status:08x}); run the Student as administrator"
        );
        let engine = Engine(handle);
        let sublayer = FWPM_SUBLAYER0 {
            subLayerKey: SUBLAYER,
            displayData: FWPM_DISPLAY_DATA0 {
                name: PWSTR(name.as_mut_ptr()),
                description: PWSTR::null(),
            },
            weight: 0xFFFF,
            ..Default::default()
        };
        // SAFETY: valid engine handle and sub-layer description.
        let status = unsafe { FwpmSubLayerAdd0(engine.0, &sublayer, None) };
        anyhow::ensure!(status == 0, "FwpmSubLayerAdd0 failed (0x{status:08x})");
        self.engine = Some(engine);
        Ok(handle)
    }
    fn add(
        &mut self,
        v6: bool,
        weight: u8,
        action: FWP_ACTION_TYPE,
        net: Option<NetCondition>,
    ) -> Result<u64> {
        let engine = self.engine()?;
        let mut name: Vec<u16> = "Classroom Guard".encode_utf16().chain(Some(0)).collect();
        let (mut v4, mut v6mask);
        let mut conditions = Vec::new();
        if let Some(net) = net {
            let mut condition = FWPM_FILTER_CONDITION0 {
                fieldKey: FWPM_CONDITION_IP_REMOTE_ADDRESS,
                matchType: FWP_MATCH_EQUAL,
                ..Default::default()
            };
            match net {
                NetCondition::V4(cidr) => {
                    v4 = FWP_V4_ADDR_AND_MASK {
                        addr: u32::from(cidr.ip),
                        mask: cidr.mask(),
                    };
                    condition.conditionValue.r#type = FWP_V4_ADDR_MASK;
                    condition.conditionValue.Anonymous.v4AddrMask = &mut v4;
                }
                NetCondition::V6(address, prefix) => {
                    v6mask = FWP_V6_ADDR_AND_MASK {
                        addr: address,
                        prefixLength: prefix,
                    };
                    condition.conditionValue.r#type = FWP_V6_ADDR_MASK;
                    condition.conditionValue.Anonymous.v6AddrMask = &mut v6mask;
                }
            }
            conditions.push(condition);
        }
        let filter = FWPM_FILTER0 {
            displayData: FWPM_DISPLAY_DATA0 {
                name: PWSTR(name.as_mut_ptr()),
                description: PWSTR::null(),
            },
            layerKey: if v6 {
                FWPM_LAYER_ALE_AUTH_CONNECT_V6
            } else {
                FWPM_LAYER_ALE_AUTH_CONNECT_V4
            },
            subLayerKey: SUBLAYER,
            weight: FWP_VALUE0 {
                r#type: FWP_UINT8,
                Anonymous: FWP_VALUE0_0 { uint8: weight },
            },
            numFilterConditions: u32::try_from(conditions.len())?,
            filterCondition: if conditions.is_empty() {
                std::ptr::null_mut()
            } else {
                conditions.as_mut_ptr()
            },
            action: FWPM_ACTION0 {
                r#type: action,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut id = 0u64;
        // SAFETY: `filter` and everything it points to (name, conditions, address
        // masks) outlive this call; WFP copies them.
        let status = unsafe { FwpmFilterAdd0(engine, &filter, None, Some(&mut id)) };
        anyhow::ensure!(status == 0, "FwpmFilterAdd0 failed (0x{status:08x})");
        Ok(id)
    }
    fn delete(&mut self, id: u64) {
        if let Some(engine) = &self.engine {
            // SAFETY: valid engine handle; an unknown id is simply reported.
            unsafe {
                let _ = FwpmFilterDeleteById0(engine.0, id);
            }
        }
    }
}
#[derive(Clone, Copy)]
enum NetCondition {
    V4(Cidr),
    V6([u8; 16], u8),
}
fn v6(address: &str, prefix: u8) -> NetCondition {
    NetCondition::V6(
        address
            .parse::<std::net::Ipv6Addr>()
            .map_or([0; 16], |a| a.octets()),
        prefix,
    )
}

impl Firewall for WfpFirewall {
    fn set_plan(&mut self, plan: &Plan) -> Result<()> {
        let engine = self.engine()?;
        // SAFETY: engine is an open WFP session, exclusively held by this
        // firewall's mutex. Atomic replacement keeps old blocks effective
        // throughout periodic endpoint refresh, including on failure.
        let status = unsafe { FwpmTransactionBegin0(engine, 0) };
        anyhow::ensure!(status == 0, "WFP transaction begin failed (0x{status:08x})");
        let result = (|| -> Result<Vec<u64>> {
            for id in &self.plan_ids {
                let status = unsafe { FwpmFilterDeleteById0(engine, *id) };
                anyhow::ensure!(
                    status == 0,
                    "WFP filter replacement failed (0x{status:08x})"
                );
            }
            let mut ids = Vec::new();
            // Teacher and upstream resolvers first: they must always get through.
            for cidr in &plan.permit {
                let essential = cidr.prefix == 32;
                let w = if essential {
                    weight::ESSENTIAL
                } else {
                    weight::PERMIT
                };
                ids.push(self.add(false, w, FWP_ACTION_PERMIT, Some(NetCondition::V4(*cidr)))?);
            }
            for cidr in &plan.block {
                ids.push(self.add(
                    false,
                    weight::BLOCK_LISTED,
                    FWP_ACTION_BLOCK,
                    Some(NetCondition::V4(*cidr)),
                )?);
            }
            for ip in &plan.endpoints {
                let (v6, net) = match ip {
                    std::net::IpAddr::V4(ip) => (
                        false,
                        NetCondition::V4(Cidr {
                            ip: *ip,
                            prefix: 32,
                        }),
                    ),
                    std::net::IpAddr::V6(ip) => (true, NetCondition::V6(ip.octets(), 128)),
                };
                // FWP_UINT8 is a range index (0..=15), not an arbitrary
                // integer weight. Use the highest range for public endpoint
                // blocks so /32 allowlist permits cannot bypass Copilot.
                ids.push(self.add(v6, weight::ENDPOINT, FWP_ACTION_BLOCK, Some(net))?);
            }
            // IPv6: local scopes stay open, everything else follows the mode.
            for (address, prefix) in [("::1", 128), ("fe80::", 10), ("fc00::", 7), ("ff00::", 8)] {
                ids.push(self.add(
                    true,
                    weight::PERMIT,
                    FWP_ACTION_PERMIT,
                    Some(v6(address, prefix)),
                )?);
            }
            if plan.default_block {
                ids.push(self.add(false, weight::DEFAULT_BLOCK, FWP_ACTION_BLOCK, None)?);
                ids.push(self.add(true, weight::DEFAULT_BLOCK, FWP_ACTION_BLOCK, None)?);
            }
            Ok(ids)
        })();
        match result {
            Ok(ids) => {
                let status = unsafe { FwpmTransactionCommit0(engine) };
                if status != 0 {
                    unsafe {
                        let _ = FwpmTransactionAbort0(engine);
                    }
                    anyhow::bail!("WFP transaction commit failed (0x{status:08x})");
                }
                self.plan_ids = ids;
                Ok(())
            }
            Err(error) => {
                unsafe {
                    let _ = FwpmTransactionAbort0(engine);
                }
                Err(error)
            }
        }
    }
    fn set_learned(&mut self, ips: &[Ipv4Addr]) -> Result<()> {
        let wanted: std::collections::HashSet<Ipv4Addr> = ips.iter().copied().collect();
        let stale: Vec<Ipv4Addr> = self
            .learned
            .keys()
            .filter(|ip| !wanted.contains(ip))
            .copied()
            .collect();
        for ip in stale {
            for id in self.learned.remove(&ip).unwrap_or_default() {
                self.delete(id);
            }
        }
        for ip in wanted {
            if !self.learned.contains_key(&ip) {
                let id = self.add(
                    false,
                    weight::PERMIT,
                    FWP_ACTION_PERMIT,
                    Some(NetCondition::V4(Cidr { ip, prefix: 32 })),
                )?;
                self.learned.insert(ip, vec![id]);
            }
        }
        Ok(())
    }
    fn clear(&mut self) {
        // Closing the dynamic session removes every filter in one step.
        self.engine = None;
        self.plan_ids.clear();
        self.learned.clear();
    }
}

fn powershell(script: &str) -> Result<String> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .creation_flags(NO_WINDOW)
        .output()
        .context("Could not run PowerShell")?;
    anyhow::ensure!(
        output.status.success(),
        "PowerShell failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

const NRPT_COMMENT: &str = "ClassroomGuard";
const NRPT_REMOVE: &str = "Get-DnsClientNrptRule | Where-Object { $_.Comment -eq 'ClassroomGuard' } | Remove-DnsClientNrptRule -Force; Clear-DnsClientCache";

pub struct NrptRedirect;
impl DnsRedirect for NrptRedirect {
    fn copilot_cached_addresses(&self) -> Vec<std::net::IpAddr> {
        powershell("Get-DnsClientCache | Where-Object { $_.Entry -match '(^|\\.)githubcopilot\\.com$|^copilot-proxy\\.githubusercontent\\.com$|^origin-tracker\\.githubusercontent\\.com$' } | ForEach-Object { $_.Data }")
            .unwrap_or_default().lines().filter_map(|s| s.trim().parse().ok()).take(256).collect()
    }
    fn upstream_servers(&self) -> Result<Vec<SocketAddr>> {
        let text = powershell(
            "Get-DnsClientServerAddress -AddressFamily IPv4 | ForEach-Object { $_.ServerAddresses } | Select-Object -Unique",
        )?;
        let servers: Vec<SocketAddr> = text
            .lines()
            .filter_map(|line| line.trim().parse::<Ipv4Addr>().ok())
            .filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
            .map(|ip| SocketAddr::new(ip.into(), 53))
            .collect();
        anyhow::ensure!(
            !servers.is_empty(),
            "No DNS server is configured on this computer"
        );
        Ok(servers)
    }
    fn install(&mut self) -> Result<()> {
        powershell(&format!(
            "{NRPT_REMOVE}; Add-DnsClientNrptRule -Namespace '.' -NameServers '127.0.0.1' -Comment '{NRPT_COMMENT}'; Clear-DnsClientCache"
        ))
        .map(|_| ())
    }
    fn flush(&mut self) {
        let _ = Command::new("ipconfig.exe")
            .arg("/flushdns")
            .creation_flags(NO_WINDOW)
            .output();
    }
    fn remove(&mut self) {
        if let Err(error) = powershell(NRPT_REMOVE) {
            tracing::warn!(%error, "Could not remove the DNS redirect rule");
        }
    }
}
