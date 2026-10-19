//! Per-user role/IP profile; CLI operations remain independent and override it.
use super::launcher::LaunchSelection;
use anyhow::{Context, Result};
#[cfg(test)]
use app_core::Role;
use std::{
    fs,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn profile_path(explicit: Option<&Path>) -> Result<PathBuf> {
    explicit
        .map(Path::to_path_buf)
        .or_else(|| super::launcher::settings_path().map(|p| p.with_file_name("profile.toml")))
        .context("cannot determine Classroom profile directory")
}
/// A document of the profile directory's database (the folder holding `path`).
/// Launch selection, device identity, Student settings and the like live here;
/// no per-feature file is read or written any more.
pub(crate) fn profile_doc(path: &Path, key: &str) -> Result<classroom_store::Doc> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    Ok(classroom_store::open_shared(directory)?.doc(classroom_store::ns::PROFILE, key))
}
pub fn load(path: &Path) -> Option<LaunchSelection> {
    let value: serde_json::Value = profile_doc(path, "launch").ok()?.read().ok()??;
    Some(LaunchSelection {
        role: value.get("role")?.as_str()?.parse().ok()?,
        teacher_ip: value
            .get("teacher_ip")?
            .as_str()?
            .parse::<Ipv4Addr>()
            .ok()?,
    })
}
pub fn save(path: &Path, selection: LaunchSelection) -> Result<()> {
    profile_doc(path, "launch")?.write(&serde_json::json!({
        "role": selection.role.to_string(),
        "teacher_ip": selection.teacher_ip.to_string(),
    }))?;
    Ok(())
}
pub fn student_identity(path: &Path, port: u16) -> Result<String> {
    let doc = profile_doc(path, "device_id")?;
    let generated = format!(
        "{}-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        std::process::id()
    );
    // Two processes starting together must agree on one identity.
    let store = classroom_store::open_shared(path.parent().unwrap_or_else(|| Path::new(".")))?;
    store.insert_if_absent(
        classroom_store::ns::PROFILE,
        "device_id",
        &serde_json::json!(generated),
    )?;
    let id: String = doc
        .read()?
        .context("device identity missing after creation")?;
    anyhow::ensure!(
        !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_digit() || c == '-'),
        "invalid saved device identity"
    );
    Ok(format!("{id}-{port}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_preserves_runtime_options_but_uses_updated_profile() {
        let args = [
            "--role=student",
            "--teacher-ip",
            "192.168.50.1",
            "--log-file",
            "session.log",
            "--launch-profile=old.toml",
            "--media-port",
            "47812",
        ];
        let actual = restart_arguments(args.into_iter().map(Into::into), Path::new("new.toml"));
        let expected: Vec<std::ffi::OsString> = [
            "--log-file",
            "session.log",
            "--media-port",
            "47812",
            "--launch-profile",
            "new.toml",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        assert_eq!(actual, expected);
    }
    #[test]
    fn view_preferences_round_trip_and_invalid_size_falls_back() {
        let folder = std::env::temp_dir().join(format!("classroom-view-{}", std::process::id()));
        let path = folder.join("profile.toml");
        let expected = UiPreferences {
            list: true,
            width: 1024,
            height: 700,
        };
        expected.save(&path).unwrap();
        assert_eq!(UiPreferences::load(&path), expected);
        profile_doc(&path, "ui_preferences")
            .unwrap()
            .write(&serde_json::json!({"list": true, "width": -1, "height": 700}))
            .unwrap();
        assert_eq!(UiPreferences::load(&path), UiPreferences::default());
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn profile_round_trip_identity_survives_relaunch_and_ports_stay_distinct() {
        let folder = std::env::temp_dir().join(format!("classroom-profile-{}", std::process::id()));
        let path = folder.join("profile.toml");
        save(
            &path,
            LaunchSelection {
                role: Role::Student,
                teacher_ip: Ipv4Addr::new(192, 168, 100, 219),
            },
        )
        .unwrap();
        let read = load(&path).unwrap();
        assert_eq!(read.role, Role::Student);
        assert_eq!(read.teacher_ip, Ipv4Addr::new(192, 168, 100, 219));
        let id = student_identity(&path, 47810).unwrap();
        assert_eq!(id, student_identity(&path, 47810).unwrap());
        assert_ne!(id, student_identity(&path, 47812).unwrap());
        profile_doc(&path, "launch")
            .unwrap()
            .write(&serde_json::json!({"role": "bad", "teacher_ip": "x"}))
            .unwrap();
        assert!(load(&path).is_none());
        fs::remove_dir_all(folder).unwrap();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiPreferences {
    pub list: bool,
    pub width: usize,
    pub height: usize,
}
impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            list: false,
            width: 1200,
            height: 760,
        }
    }
}
impl UiPreferences {
    pub fn load(profile: &Path) -> Self {
        let read = || -> Option<Self> {
            let v: serde_json::Value =
                profile_doc(profile, "ui_preferences").ok()?.read().ok()??;
            let value = Self {
                list: v.get("list")?.as_bool()?,
                width: usize::try_from(v.get("width")?.as_i64()?).ok()?,
                height: usize::try_from(v.get("height")?.as_i64()?).ok()?,
            };
            ((640..=4096).contains(&value.width) && (480..=2160).contains(&value.height))
                .then_some(value)
        };
        read().unwrap_or_default()
    }
    pub fn save(self, profile: &Path) -> Result<()> {
        profile_doc(profile, "ui_preferences")?.write(&serde_json::json!({
            "list": self.list,
            "width": self.width.clamp(640, 4096),
            "height": self.height.clamp(480, 2160),
        }))?;
        Ok(())
    }
}
pub fn restart(profile: &Path) -> Result<()> {
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command.args(restart_arguments(std::env::args_os().skip(1), profile));
    if let Some(selection) = load(profile) {
        command.arg("--role").arg(selection.role.to_string());
    }
    replace(command)
}

pub fn restart_current() -> Result<()> {
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command.args(std::env::args_os().skip(1));
    replace(command)
}

fn replace(mut command: std::process::Command) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Run only after media/runtime teardown. exec retains launchd's PID and
        // scheduling policy, and cannot leave an old and new Student competing.
        Err(command.exec()).context("cannot restart Classroom with saved settings")
    }
    #[cfg(not(unix))]
    {
        command
            .spawn()
            .context("cannot restart Classroom with saved settings")?;
        Ok(())
    }
}

fn restart_arguments(
    args: impl IntoIterator<Item = std::ffi::OsString>,
    profile: &Path,
) -> Vec<std::ffi::OsString> {
    let mut result = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if ["--teacher-ip", "--role", "--launch-profile"]
            .iter()
            .any(|key| arg == *key)
        {
            args.next();
        } else if !["--teacher-ip=", "--role=", "--launch-profile="]
            .iter()
            .any(|key| arg.to_string_lossy().starts_with(key))
        {
            result.push(arg);
        }
    }
    result.extend(["--launch-profile".into(), profile.as_os_str().to_owned()]);
    result
}

pub fn student_instance(port: u16) -> Result<Option<fs::File>> {
    use fs2::FileExt;
    let path = super::launcher::settings_path()
        .context("cannot determine Student instance directory")?
        .with_file_name(format!("student-{port}.lock"));
    fs::create_dir_all(path.parent().context("invalid Student lock path")?)?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(file)),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(error) => Err(error).context("cannot lock Student instance"),
    }
}
