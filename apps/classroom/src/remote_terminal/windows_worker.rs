//! Spawn the terminal worker with Explorer's ordinary interactive token.
//! Never fall back to an elevated child if the desktop token is unavailable.
#![allow(unsafe_code)] // Small, documented Windows token/process boundary.
use anyhow::{ensure, Context, Result};
use std::path::Path;
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Security::{
            DuplicateTokenEx, SecurityImpersonation, TokenPrimary, TOKEN_ADJUST_DEFAULT,
            TOKEN_ADJUST_SESSIONID, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_QUERY,
        },
        System::Threading::{
            CreateProcessWithTokenW, GetExitCodeProcess, OpenProcess, OpenProcessToken,
            TerminateProcess, CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, LOGON_WITH_PROFILE,
            PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
            PROCESS_TERMINATE, STARTUPINFOW,
        },
        UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId},
    },
};
pub struct Process {
    handle: Option<HANDLE>,
    normal: Option<std::process::Child>,
}
/// A process may inherit CTRL_C_IGNORE even when it has no visible console.
/// Clear it before creating ConPTY/cmd, so native console children receive ETX
/// normally. Do not synthesize signals or modify an application's raw mode.
pub fn reset_ctrl_c_inheritance() -> Result<()> {
    // SAFETY: process-local flag in this isolated ordinary-user worker. This
    // allocates no console/window and changes neither Teacher nor Student GUI.
    unsafe {
        windows::Win32::System::Console::SetConsoleCtrlHandler(None, false)?;
    }
    Ok(())
}
// SAFETY: process handles can be queried/closed on any thread; uniquely owned.
unsafe impl Send for Process {}
impl Process {
    pub fn finished(&mut self) -> bool {
        if let Some(child) = self.normal.as_mut() {
            return child.try_wait().is_ok_and(|v| v.is_some());
        }
        let mut code = 0;
        unsafe { GetExitCodeProcess(self.handle.unwrap(), &mut code).is_err() || code != 259 }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(child) = self.normal.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        let handle = self.handle.unwrap();
        unsafe {
            if !self.finished() {
                let _ = TerminateProcess(handle, 1);
            }
            let _ = CloseHandle(handle);
        }
    }
}
struct Handle(HANDLE);
// SAFETY: uniquely owned kernel handles have no thread affinity.
unsafe impl Send for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
pub fn spawn(path: &Path) -> Result<Process> {
    let exe = std::env::current_exe()?;
    if !crate::guard::windows::is_elevated() {
        use std::os::windows::process::CommandExt;
        let child = std::process::Command::new(&exe)
            .arg("--terminal-worker")
            .arg(path)
            .creation_flags(CREATE_NO_WINDOW.0)
            .spawn()?;
        return Ok(Process {
            handle: None,
            normal: Some(child),
        });
    }
    let command = format!(
        "\"{}\" --terminal-worker \"{}\"",
        exe.display(),
        path.display()
    );
    let mut command: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
    let application: Vec<u16> = exe
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: all handles are owned by RAII wrappers and all UTF-16 buffers
    // remain live through CreateProcessWithTokenW; no handle inheritance.
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(GetShellWindow(), Some(&mut pid));
        ensure!(pid != 0, "Interactive Student desktop unavailable");
        let shell = Handle(
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                .context("Open interactive user process")?,
        );
        let mut token = HANDLE::default();
        OpenProcessToken(
            shell.0,
            TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ASSIGN_PRIMARY,
            &mut token,
        )
        .context("Open interactive user token")?;
        let token = Handle(token);
        let mut primary = HANDLE::default();
        DuplicateTokenEx(
            token.0,
            TOKEN_DUPLICATE
                | TOKEN_QUERY
                | TOKEN_ASSIGN_PRIMARY
                | TOKEN_ADJUST_DEFAULT
                | TOKEN_ADJUST_SESSIONID,
            None,
            SecurityImpersonation,
            TokenPrimary,
            &mut primary,
        )
        .context("Duplicate ordinary user token")?;
        let primary = Handle(primary);
        use windows::Win32::System::Environment::{
            CreateEnvironmentBlock, DestroyEnvironmentBlock,
        };
        struct Environment(*mut std::ffi::c_void);
        impl Drop for Environment {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyEnvironmentBlock(self.0);
                }
            }
        }
        let mut environment = std::ptr::null_mut();
        CreateEnvironmentBlock(&mut environment, Some(primary.0), false)
            .context("Create ordinary user environment")?;
        let environment = Environment(environment);
        let startup = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        };
        let mut process = PROCESS_INFORMATION::default();
        CreateProcessWithTokenW(
            primary.0,
            LOGON_WITH_PROFILE,
            PCWSTR(application.as_ptr()),
            Some(PWSTR(command.as_mut_ptr())),
            CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
            Some(environment.0),
            PCWSTR::null(),
            &startup,
            &mut process,
        )
        .context("Start non-elevated terminal worker")?;
        let _thread = Handle(process.hThread);
        Ok(Process {
            handle: Some(process.hProcess),
            normal: None,
        })
    }
}

pub struct Job(Handle);
impl Job {
    pub fn for_pid(pid: u32) -> Result<Self> {
        use windows::Win32::System::JobObjects::*;
        // SAFETY: job/process handles are uniquely owned; Windows copies the
        // limit structure. The job contains only this PTY shell and children.
        unsafe {
            let job = Handle(CreateJobObjectW(None, PCWSTR::null())?);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            )?;
            let process = Handle(OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_TERMINATE,
                false,
                pid,
            )?);
            AssignProcessToJobObject(job.0, process.0)?;
            Ok(Self(job))
        }
    }
}
