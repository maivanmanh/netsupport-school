//! Student side of class-wide power and screen-lock controls. The result is
//! reported to the Teacher *before* a command that ends this session runs.
use super::Inbox;
use anyhow::{Context, Result};
use protocol::ToolPacket;
use serde_json::Value;
use std::{process::Command, sync::Arc, thread, time::Duration};
use tracing::info;

/// Grace period before restart/shutdown so the Student sees it coming.
const GRACE_SECONDS: u32 = 15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Lock,
    Unlock,
    Reboot,
    Shutdown,
    Logout,
    ClipboardBlock,
    ClipboardAllow,
}
fn parse(op: &str) -> Option<Op> {
    Some(match op {
        "lock" => Op::Lock,
        "unlock" => Op::Unlock,
        "reboot" => Op::Reboot,
        "shutdown" => Op::Shutdown,
        "logout" => Op::Logout,
        "clipboard_block" => Op::ClipboardBlock,
        "clipboard_allow" => Op::ClipboardAllow,
        _ => return None,
    })
}

#[cfg(windows)]
fn system(op: Op) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("shutdown.exe");
    match op {
        Op::Reboot => command.args(["/r", "/f", "/t", &GRACE_SECONDS.to_string(), "/c", "Restarting: requested by your teacher"]),
        Op::Shutdown => command.args(["/s", "/f", "/t", &GRACE_SECONDS.to_string(), "/c", "Shutting down: requested by your teacher"]),
        _ => command.arg("/l"),
    };
    command.creation_flags(0x0800_0000);
    let status = command.status().context("Could not run shutdown.exe")?;
    anyhow::ensure!(status.success(), "shutdown.exe failed ({status})");
    Ok(())
}
#[cfg(not(windows))]
fn system(op: Op) -> Result<()> {
    // loginwindow handles these events without a confirmation sheet.
    let event = match op {
        Op::Reboot => "aevtrrst",
        Op::Shutdown => "aevtrsdn",
        _ => "aevtrlgo",
    };
    if matches!(op, Op::Reboot | Op::Shutdown) {
        thread::sleep(Duration::from_secs(u64::from(GRACE_SECONDS)));
    }
    let status = Command::new("osascript")
        .args(["-e", &format!("tell application \"loginwindow\" to «event {event}»")])
        .status()
        .context("Could not run osascript")?;
    anyhow::ensure!(
        status.success(),
        "macOS refused the request; allow Classroom Student to control loginwindow in Privacy & Security → Automation"
    );
    Ok(())
}

pub(super) fn run(p: ToolPacket, inbox: Arc<Inbox>) {
    let request: Value = serde_json::from_str(&p.text).unwrap_or(Value::Null);
    let Some(op) = request["op"].as_str().and_then(parse) else {
        inbox.event(&p.id, "error", "Unknown power action", 0);
        return;
    };
    match op {
        Op::Lock => {
            let message = request["message"].as_str().unwrap_or("").trim().to_owned();
            inbox.set_lock(Some(message));
            info!(job_id=%p.id,"Student screen locked by Teacher");
            inbox.event(&p.id, "complete", "", 0);
        }
        Op::Unlock => {
            inbox.set_lock(None);
            info!(job_id=%p.id,"Student screen unlocked by Teacher");
            inbox.event(&p.id, "complete", "", 0);
        }
        Op::ClipboardBlock | Op::ClipboardAllow => {
            match inbox.set_clipboard_block(op == Op::ClipboardBlock) {
                Ok(()) => {
                    info!(job_id=%p.id,blocked=(op == Op::ClipboardBlock),"Student clipboard control changed by Teacher");
                    inbox.event(&p.id, "complete", "", 0);
                }
                Err(error) => inbox.event(&p.id, "error", &format!("{error:#}"), 0),
            }
        }
        Op::Reboot | Op::Shutdown | Op::Logout => {
            // Report first; the command may end this process and connection.
            inbox.event(&p.id, "complete", "", 0);
            let id = p.id;
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(2500));
                info!(job_id=%id,?op,"Student executing power action");
                if let Err(error) = system(op) {
                    info!(job_id=%id,%error,"Student power action failed");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_operations_parse() {
        assert_eq!(parse("lock"), Some(Op::Lock));
        assert_eq!(parse("logout"), Some(Op::Logout));
        assert_eq!(parse("rm -rf"), None);
    }

    #[test]
    fn lock_and_unlock_change_the_inbox_without_touching_the_system() {
        let inbox = Arc::new(Inbox::default());
        let lock = |op: &str, message: &str| {
            let mut p = ToolPacket::default();
            p.id = "job".into();
            p.text = serde_json::json!({"op":op,"message":message}).to_string();
            run(p, Arc::clone(&inbox));
        };
        lock("lock", " Eyes on the board ");
        assert_eq!(inbox.lock_state().1.as_deref(), Some("Eyes on the board"));
        lock("unlock", "");
        assert!(inbox.lock_state().1.is_none());
    }
}
