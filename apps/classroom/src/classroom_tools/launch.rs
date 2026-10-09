//! Student side of "launch an application on every computer": starts one
//! program (or opens one URL/document) in the signed-in user's session and
//! reports the outcome to the Teacher. Nothing here waits for the program.
use super::Inbox;
use anyhow::{Context, Result};
use protocol::ToolPacket;
use serde_json::Value;
use std::{path::Path, process::Command, sync::Arc, thread};
use tracing::info;

/// File name shown in Teacher status lines.
pub fn display_name(path: &str) -> String {
    let name = path
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path);
    name.chars().take(80).collect()
}

/// Replaces `%NAME%` with the environment value (unknown names stay as typed).
fn expand_env(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                match std::env::var(&after[..end]) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => out.push_str(&rest[start..start + end + 2]),
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Shell-like split honouring single/double quotes (macOS/Unix launches).
#[cfg_attr(windows, allow(dead_code))]
fn split_args(text: &str) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in text.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (_, c) => current.push(c),
        }
    }
    anyhow::ensure!(quote.is_none(), "Unbalanced quote in arguments");
    if started || !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

fn is_url(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

pub(super) fn validate_website(url: &str) -> Result<()> {
    anyhow::ensure!(
        url.len() <= 4096
            && is_url(url)
            && !url.chars().any(|c| c.is_control() || c.is_whitespace()),
        "Enter an http:// or https:// website URL"
    );
    let authority = url
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or(""))
        .unwrap_or("");
    anyhow::ensure!(
        !authority.is_empty() && !authority.contains('@'),
        "Website host is missing or contains credentials"
    );
    Ok(())
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn open_website(url: &str) -> Result<()> {
    use windows::{
        core::{w, PCWSTR},
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };
    validate_website(url)?;
    let wide: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    // All UTF-16 arguments are NUL terminated and remain alive for this call.
    // ShellExecute routes directly to the user's default browser; no command
    // interpreter or environment expansion touches URL query/percent escapes.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    anyhow::ensure!(
        result.0 as isize > 32,
        "Windows could not open the default browser (code {})",
        result.0 as isize
    );
    Ok(())
}

#[cfg(not(windows))]
fn open_website(url: &str) -> Result<()> {
    validate_website(url)?;
    let opener = if cfg!(target_os = "macos") {
        "/usr/bin/open"
    } else {
        "xdg-open"
    };
    let status = Command::new(opener)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("Could not open the default browser")?;
    anyhow::ensure!(status.success(), "Default browser refused the website");
    Ok(())
}
#[cfg(windows)]
fn has_separator(path: &str) -> bool {
    path.contains(['/', '\\'])
}

#[cfg(windows)]
fn spawn(path: &str, args: &str) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    let expanded = expand_env(path);
    let target = Path::new(&expanded);
    let mut command;
    if target.is_file() {
        command = Command::new(target);
        if let Some(folder) = target.parent().filter(|p| p.is_dir()) {
            command.current_dir(folder);
        }
        if !args.is_empty() {
            command.raw_arg(expand_env(args));
        }
    } else if is_url(&expanded) || !has_separator(&expanded) {
        // Bare names (chrome, notepad, calc) and URLs resolve through the
        // shell, which also knows the registered "App Paths".
        command = Command::new("cmd");
        command.raw_arg(format!(
            "/D /C start \"\" \"{expanded}\" {}",
            expand_env(args)
        ));
        command.creation_flags(NO_WINDOW);
    } else {
        anyhow::bail!("Program not found: {expanded}");
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("Could not start {expanded}"))?;
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(not(windows))]
fn spawn(path: &str, args: &str) -> Result<()> {
    let expanded = expand_env(path);
    let expanded = match expanded.strip_prefix("~/") {
        Some(rest) => super::user_folder("")?
            .join(rest)
            .to_string_lossy()
            .into_owned(),
        None => expanded,
    };
    let list = split_args(&expand_env(args))?;
    let target = Path::new(&expanded);
    let mut command;
    if is_url(&expanded) {
        command = Command::new("open");
        command.arg(&expanded);
    } else if target.is_dir() && cfg!(target_os = "macos") {
        command = Command::new("open");
        command.arg("-a").arg(target);
        if !list.is_empty() {
            command.arg("--args").args(&list);
        }
    } else if target.is_file() {
        command = Command::new(target);
        command.args(&list);
    } else {
        anyhow::bail!("Program not found: {expanded}");
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command
        .spawn()
        .with_context(|| format!("Could not start {expanded}"))?;
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Starts the requested program off the poll loop and sends the outcome.
pub(super) fn run(p: ToolPacket, inbox: Arc<Inbox>) {
    thread::spawn(move || {
        let outcome = serde_json::from_str::<Value>(&p.text)
            .context("Invalid launch request")
            .and_then(|request| {
                if p.action == "website" {
                    return open_website(request["url"].as_str().unwrap_or(""));
                }
                spawn(
                    request["path"].as_str().unwrap_or(""),
                    request["args"].as_str().unwrap_or(""),
                )
            });
        match outcome {
            Ok(()) => {
                info!(job_id=%p.id,program=%p.name,"Student launched requested program");
                inbox.event(&p.id, "complete", "", 0);
            }
            Err(error) => {
                info!(job_id=%p.id,%error,"Student could not launch requested program");
                inbox.event(&p.id, "error", &format!("{error:#}"), 0);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_arguments_are_parsed() {
        assert_eq!(
            display_name(r"C:\Program Files\Google\chrome.exe"),
            "chrome.exe"
        );
        assert_eq!(
            display_name("/Applications/Google Chrome.app/"),
            "Google Chrome.app"
        );
        assert_eq!(
            split_args(r#"--new-window "https://a.b/c d" 'x y' ''"#).unwrap(),
            ["--new-window", "https://a.b/c d", "x y", ""]
        );
        assert!(split_args("\"open").is_err());
        assert!(split_args("   ").unwrap().is_empty());
    }

    #[test]
    fn environment_references_expand_only_when_defined() {
        std::env::set_var("CLASSROOM_LAUNCH_TEST", "ok");
        assert_eq!(
            expand_env("%CLASSROOM_LAUNCH_TEST%\\x %NOPE_UNSET% 100%"),
            "ok\\x %NOPE_UNSET% 100%"
        );
    }

    #[test]
    fn hub_requires_capability_and_a_program_path() {
        use super::super::Hub;
        use serde_json::json;
        let hub = Arc::new(Hub::default());
        let folder = std::env::temp_dir();
        {
            let mut st = hub.0.lock().unwrap();
            st.peers.insert("old".into(), 1);
            st.peers.insert("new".into(), 1);
            st.capabilities.insert("new".into(), json!({"launch":1}));
        }
        let input = json!({"path":"C:/Program Files/Google/Chrome/Application/chrome.exe","args":"https://example.org"});
        let refused = hub
            .submit("launch", &["old".into()], &input, &folder)
            .unwrap_err();
        assert!(refused.to_string().contains("older build"));
        assert!(hub
            .submit("launch", &["new".into()], &json!({"path":"  "}), &folder)
            .is_err());
        let id = hub
            .submit("launch", &["new".into()], &input, &folder)
            .unwrap();
        let queued = hub.0.lock().unwrap().queues["new"][0].clone();
        assert_eq!(
            (
                queued.action.as_str(),
                queued.id.as_str(),
                queued.name.as_str()
            ),
            ("launch", id.as_str(), "chrome.exe")
        );
        assert!(queued.text.contains("https://example.org"));
    }

    #[test]
    fn policy_jobs_need_a_valid_document_the_guard_capability_filter_is_the_callers_job() {
        // The "apply where possible, skip and report the rest" capability
        // filter now lives in the Teacher agent (one computer lacking the
        // `guard` capability must not block the whole batch); Hub::submit()
        // itself only validates the document and queues it for whoever the
        // caller already decided is a recipient.
        use super::super::Hub;
        use serde_json::json;
        let hub = Arc::new(Hub::default());
        let folder = std::env::temp_dir();
        {
            let mut st = hub.0.lock().unwrap();
            st.peers.insert("guarded".into(), 1);
            st.capabilities.insert("guarded".into(), json!({"guard":1}));
        }
        let policy = json!({"policy":{"internet":{"mode":"allow_only","rules":["*.wikipedia.org"]},"lease_minutes":30}});
        let empty = json!({"policy":{"internet":{"mode":"allow_only","rules":[]}}});
        assert!(hub
            .submit("policy", &["guarded".into()], &empty, &folder)
            .is_err());
        let id = hub
            .submit("policy", &["guarded".into()], &policy, &folder)
            .unwrap();
        let queued = hub.0.lock().unwrap().queues["guarded"][0].clone();
        assert_eq!(
            (
                queued.action.as_str(),
                queued.id.as_str(),
                queued.name.as_str()
            ),
            ("policy", id.as_str(), "allow_only")
        );
        // A completed "open" job clears the Teacher's restricted marker.
        hub.0
            .lock()
            .unwrap()
            .internet
            .insert("guarded".into(), "allow_only".into());
        let open = hub
            .submit(
                "policy",
                &["guarded".into()],
                &json!({"policy":{"internet":{"mode":"open"}}}),
                &folder,
            )
            .unwrap();
        let mut done = ToolPacket::default();
        done.id = open;
        done.status = "complete".into();
        hub.event("guarded", &done).unwrap();
        assert!(hub.snapshot()["internet"]["guarded"].is_null());
    }

    #[test]
    fn vscode_policy_jobs_need_a_valid_document_and_track_the_blocked_marker() {
        use super::super::Hub;
        use serde_json::json;
        let hub = Arc::new(Hub::default());
        let folder = std::env::temp_dir();
        {
            let mut st = hub.0.lock().unwrap();
            st.peers.insert("guarded".into(), 1);
            st.capabilities.insert("guarded".into(), json!({"guard":1}));
        }
        let bad = json!({"policy":{"enabled":true,"allowed":["no-dot"]}});
        assert!(hub
            .submit("vscode_policy", &["guarded".into()], &bad, &folder)
            .is_err());
        let policy = json!({"policy":{"enabled":true,"allowed":["ms-python.python"]}});
        let id = hub
            .submit("vscode_policy", &["guarded".into()], &policy, &folder)
            .unwrap();
        let queued = hub.0.lock().unwrap().queues["guarded"][0].clone();
        assert_eq!(
            (
                queued.action.as_str(),
                queued.id.as_str(),
                queued.name.as_str()
            ),
            ("vscode_policy", id.as_str(), "vscode_block")
        );
        let mut done = ToolPacket::default();
        done.id = id;
        done.status = "complete".into();
        hub.event("guarded", &done).unwrap();
        assert!(hub.snapshot()["vscode_blocked"]
            .as_array()
            .unwrap()
            .contains(&json!("guarded")));
        let clear = json!({"policy":{"enabled":false,"allowed":[]}});
        let id2 = hub
            .submit("vscode_policy", &["guarded".into()], &clear, &folder)
            .unwrap();
        let mut done2 = ToolPacket::default();
        done2.id = id2;
        done2.status = "complete".into();
        hub.event("guarded", &done2).unwrap();
        assert!(hub.snapshot()["vscode_blocked"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn missing_programs_report_an_error_and_real_ones_start() {
        assert!(spawn("/definitely/missing/app", "").is_err());
        spawn("/usr/bin/true", "--ignored 'args here'").unwrap();
    }
}
