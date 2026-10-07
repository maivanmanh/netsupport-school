//! Windows Service that supervises the Student process so a standard-user
//! Student cannot simply end-task their way out of classroom enforcement
//! (Internet Guard, VS Code policy, …) — the same "kill it, it comes right
//! back" behaviour NetSupport School and endpoint-management/antivirus
//! agents show. The install step genuinely needs an administrator (the
//! project's standing assumption, see docs/21_GUARD_ARCHITECTURE.md): the
//! Student never self-elevates, an admin sets this up once before Students
//! use the machine, same as today's elevated-launch model, just made
//! self-healing and much harder to turn off.
//!
//! What this gives, concretely:
//! - The service itself runs as `LocalSystem`, auto-starts with Windows,
//!   and its own access-control list denies Stop/Pause/`WRITE_DAC` to
//!   ordinary (non-administrator) users — `sc stop`, Services.msc, and
//!   PowerShell's `Stop-Service` all fail with Access Denied for a Student.
//! - It registers (and keeps re-registering, in case a Student deletes it)
//!   a `RunLevel Highest` / `LogonType Interactive` scheduled task that
//!   launches `ClassroomStudent.exe` into the active console session —
//!   the exact mechanism already used and verified for manual deploys this
//!   session, now automated.
//! - Every couple of seconds it checks whether `ClassroomStudent.exe` is
//!   running; if a Student ends that task, it is relaunched within a few
//!   seconds via the same scheduled task.
//! - The install folder gets an explicit deny ACE (delete/write/change-DACL)
//!   for the Authenticated Users group, on top of Program Files' own
//!   default protection, so the binaries can't be removed or swapped out
//!   either.
//!
//! What this does *not* do (documented, not hidden): it does not add a
//! deny-PROCESS_TERMINATE ACL to the live Student process itself, so a
//! Student *can* still end-task the visible window for a few seconds at a
//! time — the protection is "it always comes back and the service that
//! brings it back cannot itself be stopped," not "literally un-killable."
//! It also assumes the Student account is a standard (non-administrator)
//! user, per the project's existing assumption; nothing here defends
//! against a Student who has local administrator rights.
#![cfg_attr(windows, allow(unsafe_code))]

#[cfg(windows)]
mod imp {
    use anyhow::{Context, Result};
    use std::{
        ffi::{OsStr, OsString},
        os::windows::process::CommandExt,
        path::{Path, PathBuf},
        process::Command,
        sync::mpsc,
        time::Duration,
    };
    use windows_service::{
        service::{
            ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState,
            ServiceStatus, ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
        service_manager::{ServiceManager, ServiceManagerAccess},
    };

    const SERVICE_NAME: &str = "ClassroomGuardService";
    const DISPLAY_NAME: &str = "Classroom Guard";
    const TASK_NAME: &str = "ClassroomGuardLaunch";
    const NO_WINDOW: u32 = 0x0800_0000;
    const POLL: Duration = Duration::from_secs(2);

    pub fn main() -> Result<()> {
        let path = crate::compact_log::path_for("guard-service", None)
            .context("Could not determine the service log path")?;
        // Held for the whole call so install/uninstall and the long-running
        // service loop are all observable; the Service Control Manager gives
        // a running service no console to print to.
        let _log_guard = crate::compact_log::init("info", &path).ok();
        let args: Vec<String> = std::env::args().skip(1).collect();
        match args.first().map(String::as_str) {
            Some("--install") => install(&args[1..]),
            Some("--uninstall") => uninstall(),
            Some(other) => anyhow::bail!("Unknown argument '{other}'; use --install or --uninstall"),
            None => run_as_service(),
        }
    }

    /// `ClassroomStudent.exe`'s own CLI arguments, chosen once at install
    /// time (e.g. `--teacher-ip 172.16.48.237 --control-port 47800`) and
    /// persisted next to the service so every future relaunch — including
    /// ones the running service triggers on its own, long after install —
    /// reuses them without needing to be told again. Absent entirely, the
    /// Student falls back to its own ordinary defaults (a persisted
    /// profile from a previous run, or its interactive setup).
    fn launch_args_path() -> Result<PathBuf> {
        Ok(exe_dir()?.join("student-launch-args.txt"))
    }
    fn saved_launch_args() -> Vec<String> {
        std::fs::read_to_string(launch_args_path().unwrap_or_default())
            .ok()
            .map(|text| text.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    fn exe_dir() -> Result<PathBuf> {
        Ok(std::env::current_exe()?
            .parent()
            .context("Service executable has no parent folder")?
            .to_path_buf())
    }

    fn install(student_args: &[String]) -> Result<()> {
        let dir = exe_dir()?;
        let service_exe = dir.join("ClassroomGuardService.exe");
        let student_exe = dir.join("ClassroomStudent.exe");
        anyhow::ensure!(
            service_exe.exists() && student_exe.exists(),
            "Install from the folder containing both ClassroomGuardService.exe and ClassroomStudent.exe"
        );
        if !student_args.is_empty() {
            std::fs::write(launch_args_path()?, student_args.join(" "))
                .context("Could not save the Student's launch arguments")?;
        }
        lock_down_folder(&dir);

        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)
            .context("Could not open the Service Control Manager (run this as administrator)")?;
        // A previous install's service must be removed first: create_service
        // fails outright if the name already exists.
        if let Ok(existing) = manager.open_service(SERVICE_NAME, ServiceAccess::STOP | ServiceAccess::DELETE) {
            let _ = existing.stop();
            std::thread::sleep(Duration::from_secs(1));
            let _ = existing.delete();
        }
        let info = ServiceInfo {
            name: OsString::from(SERVICE_NAME),
            display_name: OsString::from(DISPLAY_NAME),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: service_exe,
            launch_arguments: vec![],
            dependencies: vec![],
            account_name: None, // LocalSystem
            account_password: None,
        };
        let service = manager
            .create_service(&info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START)
            .context("Could not register the Classroom Guard service")?;
        let _ = service.set_description("Keeps the Classroom Student running for the active class session.");
        drop(service);

        harden_service_acl().context("Could not lock the service down against non-administrators")?;
        configure_service_recovery().context("Could not configure service auto-restart")?;
        if let Err(error) = register_launch_task(&student_exe) {
            // Not fatal: the running service retries this on every poll.
            tracing::warn!(%error, "Could not register the Student launch task during install");
        }

        let service = manager
            .open_service(SERVICE_NAME, ServiceAccess::START)
            .context("Could not reopen the service to start it")?;
        service
            .start::<&OsStr>(&[])
            .context("Could not start the Classroom Guard service")?;
        println!("Classroom Guard service installed and started.");
        Ok(())
    }

    fn uninstall() -> Result<()> {
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .context("Could not open the Service Control Manager (run this as administrator)")?;
        if let Ok(service) = manager.open_service(SERVICE_NAME, ServiceAccess::STOP | ServiceAccess::DELETE) {
            let _ = service.stop();
            std::thread::sleep(Duration::from_secs(1));
            service.delete().context("Could not remove the Classroom Guard service")?;
        }
        let _ = Command::new("schtasks.exe")
            .args(["/Delete", "/TN", TASK_NAME, "/F"])
            .creation_flags(NO_WINDOW)
            .output();
        println!("Classroom Guard service removed.");
        Ok(())
    }

    windows_service::define_windows_service!(ffi_service_main, service_main);

    fn run_as_service() -> Result<()> {
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)
            .context("Could not start the service control dispatcher (launched outside the Service Control Manager?)")
    }

    fn service_main(_arguments: Vec<OsString>) {
        if let Err(error) = run_service() {
            tracing::error!(%error, "Classroom Guard service stopped with an error");
        }
    }

    fn run_service() -> Result<()> {
        let (stop_tx, stop_rx) = mpsc::channel();
        let handler = move |control| -> ServiceControlHandlerResult {
            match control {
                windows_service::service::ServiceControl::Stop
                | windows_service::service::ServiceControl::Shutdown => {
                    let _ = stop_tx.send(());
                    ServiceControlHandlerResult::NoError
                }
                windows_service::service::ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };
        let status_handle = service_control_handler::register(SERVICE_NAME, handler)
            .context("Could not register the service control handler")?;
        let set_status = |state, accept| {
            let _ = status_handle.set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: state,
                controls_accepted: accept,
                exit_code: windows_service::service::ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            });
        };
        set_status(
            ServiceState::StartPending,
            windows_service::service::ServiceControlAccept::empty(),
        );
        let student_exe = exe_dir()?.join("ClassroomStudent.exe");
        set_status(ServiceState::Running, windows_service::service::ServiceControlAccept::STOP);
        tracing::info!("Classroom Guard service running");
        loop {
            if stop_rx.recv_timeout(POLL).is_ok() {
                break;
            }
            if let Err(error) = ensure_student_running(&student_exe) {
                tracing::warn!(%error, "Could not ensure the Student process is running");
            }
        }
        set_status(ServiceState::Stopped, windows_service::service::ServiceControlAccept::empty());
        Ok(())
    }

    fn student_running() -> bool {
        Command::new("tasklist.exe")
            .args(["/FI", "IMAGENAME eq ClassroomStudent.exe", "/FO", "CSV", "/NH"])
            .creation_flags(NO_WINDOW)
            .output()
            .is_ok_and(|o| {
                String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains("classroomstudent.exe")
            })
    }

    fn ensure_student_running(student_exe: &Path) -> Result<()> {
        if student_running() {
            return Ok(());
        }
        // Idempotent: re-registering every time the Student is found missing
        // means a Student who deletes the scheduled task does not get away
        // with it — the next poll puts it back before relaunching through it.
        if let Err(error) = register_launch_task(student_exe) {
            tracing::warn!(%error, "Could not (re)register the Student launch task");
        }
        let output = Command::new("schtasks.exe")
            .args(["/Run", "/TN", TASK_NAME])
            .creation_flags(NO_WINDOW)
            .output()
            .context("Could not run schtasks.exe")?;
        anyhow::ensure!(
            output.status.success(),
            "schtasks /Run failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        tracing::info!("Classroom Guard relaunched the Student process");
        Ok(())
    }

    fn register_launch_task(student_exe: &Path) -> Result<()> {
        let user = active_console_user()?;
        let dir = student_exe.parent().context("Student executable has no parent folder")?;
        let saved = saved_launch_args();
        let argument_line = if saved.is_empty() {
            String::new()
        } else {
            // PowerShell single-quoted string: double an embedded quote to escape it.
            let escaped = saved.join(" ").replace('\'', "''");
            format!(" -Argument '{escaped}'")
        };
        let script = format!(
            r"$action = New-ScheduledTaskAction -Execute '{exe}'{argument_line} -WorkingDirectory '{dir}'
$principal = New-ScheduledTaskPrincipal -UserId '{user}' -LogonType Interactive -RunLevel Highest
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit 0
Register-ScheduledTask -TaskName '{task}' -Action $action -Principal $principal -Settings $settings -Force | Out-Null",
            exe = student_exe.display(),
            dir = dir.display(),
            user = user,
            task = TASK_NAME,
        );
        powershell(&script)
    }

    /// The username of whoever is logged into the active console session —
    /// the scheduled task must run as that specific user to land on their
    /// visible desktop, not an arbitrary or stale one.
    fn active_console_user() -> Result<String> {
        // query.exe's exit code is not a reliable success signal (it is
        // commonly non-zero even when it printed a perfectly good session
        // list) — parse stdout regardless of the status.
        let output = Command::new("query.exe")
            .arg("user")
            .creation_flags(NO_WINDOW)
            .output()
            .context("Could not run query.exe")?;
        let text = String::from_utf8_lossy(&output.stdout);
        let user = text
            .lines()
            .find(|line| line.contains("console"))
            .or_else(|| text.lines().nth(1))
            .and_then(|line| line.trim_start_matches('>').split_whitespace().next())
            .context("Could not determine the logged-on console user")?;
        Ok(user.to_owned())
    }

    fn powershell(script: &str) -> Result<()> {
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(NO_WINDOW)
            .output()
            .context("Could not run powershell.exe")?;
        anyhow::ensure!(
            output.status.success(),
            "powershell.exe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(())
    }

    fn harden_service_acl() -> Result<()> {
        // Authenticated Users (AU) keep only query/read rights (CC LC SW LO
        // CR RC); SYSTEM (SY) and built-in Administrators (BA) keep full
        // control (including WP = SERVICE_STOP, WD = WRITE_DAC). A
        // standard-user Student therefore gets Access Denied from `sc stop`,
        // Services.msc, and PowerShell's `Stop-Service` — this exact SDDL
        // shape is Microsoft's own documented pattern for hardening a
        // service's access control (security baselines / sc sdset guidance).
        const SDDL: &str = "D:(A;;CCLCSWLOCRRC;;;IU)(A;;CCLCSWLOCRRC;;;SU)(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;AU)";
        let output = Command::new("sc.exe")
            .args(["sdset", SERVICE_NAME, SDDL])
            .creation_flags(NO_WINDOW)
            .output()
            .context("Could not run sc.exe")?;
        anyhow::ensure!(
            output.status.success(),
            "sc sdset failed: {}",
            String::from_utf8_lossy(&output.stdout).trim()
        );
        Ok(())
    }

    fn configure_service_recovery() -> Result<()> {
        // Defense in depth on top of the scheduled-task-based Student
        // supervision above: if the SERVICE process itself ever crashes
        // (not just the Student it supervises), Windows restarts it too.
        let output = Command::new("sc.exe")
            .args([
                "failure",
                SERVICE_NAME,
                "reset=",
                "86400",
                "actions=",
                "restart/5000/restart/5000/restart/5000",
            ])
            .creation_flags(NO_WINDOW)
            .output()
            .context("Could not run sc.exe")?;
        anyhow::ensure!(
            output.status.success(),
            "sc failure failed: {}",
            String::from_utf8_lossy(&output.stdout).trim()
        );
        Ok(())
    }

    /// Best-effort: a failure here still leaves the service's own ACL (the
    /// real line of defense) and Program Files' default NTFS protection in
    /// place, so it is logged, not escalated.
    fn lock_down_folder(dir: &Path) {
        // Deliberately an explicit allow-list, not a deny rule: "Authenticated
        // Users" (the group a naive deny ACE would target) includes
        // administrators too, since it means "anyone who logged in" — a
        // DENY entry for it would have blocked the very admin account doing
        // this install (confirmed the hard way: Copy-Item onto this folder
        // started failing silently for the admin account after an earlier
        // version of this function used exactly that deny). Resetting
        // inheritance and granting only SYSTEM and built-in Administrators
        // (both locale-independent well-known SIDs) means nobody else has
        // any entry at all — no ACE, no access — which is simpler and
        // cannot accidentally lock out the admin who needs to reinstall or
        // update this folder later.
        let output = Command::new("icacls.exe")
            .args([
                dir.to_string_lossy().as_ref(),
                "/inheritance:r",
                "/grant:r",
                "*S-1-5-18:(OI)(CI)F", // SYSTEM
                "/grant:r",
                "*S-1-5-32-544:(OI)(CI)F", // BUILTIN\Administrators
            ])
            .creation_flags(NO_WINDOW)
            .output();
        if let Err(error) = output {
            tracing::warn!(%error, "Could not lock the install folder down against non-administrators");
        }
    }
}

#[cfg(windows)]
pub use imp::main;

#[cfg(not(windows))]
pub fn main() -> anyhow::Result<()> {
    anyhow::bail!("The Classroom Guard service is only available on Windows")
}
