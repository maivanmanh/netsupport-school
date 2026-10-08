//! Short-lived permission checks run from the bundled helper's own identity.
//! Never edits TCC, network preferences or other applications' grants.
#![allow(unsafe_code)]
#[cfg(target_os = "macos")]
pub fn early_operation() -> Option<anyhow::Result<()>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let operation = args.first()?.as_str();
    if !matches!(
        operation,
        "--permission-status" | "--permission-request-screen" | "--permission-probe-network"
    ) {
        return None;
    }
    Some(run(&args))
}
#[cfg(not(target_os = "macos"))]
pub fn early_operation() -> Option<anyhow::Result<()>> {
    None
}

#[cfg(target_os = "macos")]
fn run(args: &[String]) -> anyhow::Result<()> {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }
    // SAFETY: Apple's process-local functions take no pointers and own consent.
    let granted = unsafe {
        if args[0] == "--permission-request-screen" {
            CGRequestScreenCaptureAccess()
        } else {
            CGPreflightScreenCaptureAccess()
        }
    };
    let mut value = serde_json::json!({"screen": granted});
    if args[0] == "--permission-probe-network" {
        let address: std::net::SocketAddrV4 = args
            .get(1)
            .ok_or_else(|| anyhow::anyhow!("IPv4 endpoint required"))?
            .parse()?;
        anyhow::ensure!(
            !address.ip().is_loopback()
                && !address.ip().is_unspecified()
                && !address.ip().is_multicast()
                && !address.ip().is_broadcast()
                && address.ip().is_private()
                && address.port() != 0,
            "Choose a private LAN IPv4 address and a non-zero TCP port"
        );
        value["network"] = match std::net::TcpStream::connect_timeout(
            &address.into(),
            std::time::Duration::from_secs(3),
        ) {
            Ok(_) => serde_json::json!({"state":"connected"}),
            Err(e) => serde_json::json!({"state":"unverified", "detail":e.to_string()}),
        };
    }
    println!("{value}");
    Ok(())
}
