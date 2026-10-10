//! Windows CMD compatibility commands are PATH-tail helpers, never PTY text
//! rewriting. CMD retains ownership of editing, quotes, pipes and redirection.
//! Existing builtins, DOSKEY macros, current-directory and PATH programs win.
#![allow(unsafe_code)] // Literal-target ShellExecuteW boundary is documented below.
use anyhow::{bail, ensure, Context, Result};
use std::{
    fs,
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};

#[cfg_attr(not(any(windows, test)), allow(dead_code))]
const COMMANDS: &[&str] = &[
    "ls", "clear", "pwd", "cat", "cp", "mv", "rm", "touch", "head", "tail", "grep", "which",
    "uname", "ps", "kill", "env", "printenv", "export", "history", "man", "sleep", "du", "df",
    "wc", "basename", "dirname", "realpath", "open",
];

/// A helper never evaluates argument text as shell code. Unsupported switches
/// fail explicitly; destructive commands validate their whole plan first.
#[derive(Debug, PartialEq)]
struct Args {
    flags: Vec<String>,
    paths: Vec<String>,
}
fn arguments(input: &[String], accepted: &[&str]) -> Result<Args> {
    let mut flags = Vec::new();
    let mut paths = Vec::new();
    let mut options = true;
    for value in input {
        if options && value == "--" {
            options = false;
            continue;
        }
        if options && value.starts_with('-') && value != "-" {
            if accepted.contains(&value.as_str()) {
                flags.push(value.clone());
            } else if value.starts_with("--") {
                bail!("Unsupported option: {value}");
            } else {
                for c in value[1..].chars() {
                    let f = format!("-{c}");
                    ensure!(accepted.contains(&f.as_str()), "Unsupported option: {f}");
                    flags.push(f);
                }
            }
        } else {
            paths.push(value.clone());
        }
    }
    Ok(Args { flags, paths })
}
fn no_glob(path: &str) -> Result<()> {
    ensure!(
        !path.contains(['*', '?']),
        "Wildcard form is ambiguous; use explicit paths or the native CMD command"
    );
    Ok(())
}
fn regular(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            fs::symlink_metadata(path)?.file_attributes() & 0x400 == 0,
            "Reparse points are not followed by compatibility file operations"
        );
    }
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "Symbolic links are not followed by this compatibility operation"
    );
    Ok(())
}
fn status(program: &str, args: &[String]) -> Result<i32> {
    Ok(Command::new(native_program(program))
        .args(args)
        .status()?
        .code()
        .unwrap_or(1))
}
fn one(paths: &[String]) -> Result<&str> {
    ensure!(paths.len() == 1, "Expected one argument");
    Ok(&paths[0])
}
fn paths_or_stdin(paths: &[String]) -> Result<Box<dyn Read>> {
    ensure!(paths.len() <= 1, "Expected zero or one input file");
    Ok(if let Some(p) = paths.first() {
        Box::new(fs::File::open(p)?)
    } else {
        Box::new(io::stdin())
    })
}
fn same_file(a: &Path, b: &Path) -> Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let x = fs::metadata(a)?;
        let y = fs::metadata(b)?;
        Ok(x.dev() == y.dev() && x.ino() == y.ino())
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION},
        };
        let x = fs::File::open(a)?;
        let y = fs::File::open(b)?;
        let mut xi = BY_HANDLE_FILE_INFORMATION::default();
        let mut yi = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: both handles are borrowed from live owned files; structures
        // are correctly sized and retained throughout these read-only queries.
        unsafe {
            GetFileInformationByHandle(HANDLE(x.as_raw_handle()), &mut xi)?;
            GetFileInformationByHandle(HANDLE(y.as_raw_handle()), &mut yi)?;
        }
        Ok(xi.dwVolumeSerialNumber == yi.dwVolumeSerialNumber
            && xi.nFileIndexHigh == yi.nFileIndexHigh
            && xi.nFileIndexLow == yi.nFileIndexLow)
    }
}
fn copy_tree(src: &Path, dst: &Path, recursive: bool) -> Result<()> {
    regular(src)?;
    if dst.exists() {
        regular(dst)?;
        if src.is_file() && dst.is_file() {
            ensure!(
                !same_file(src, dst)?,
                "Source and destination refer to the same file"
            );
        }
    }
    if src.is_dir() {
        ensure!(recursive, "Directory requires -r or -R");
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_tree(&entry.path(), &dst.join(entry.file_name()), true)?;
        }
    } else {
        if dst.exists() {
            regular(dst)?;
        }
        fs::copy(src, dst)?;
    }
    Ok(())
}
fn validate_tree(src: &Path) -> Result<()> {
    regular(src)?;
    if src.is_dir() {
        for e in fs::read_dir(src)? {
            validate_tree(&e?.path())?;
        }
    }
    Ok(())
}
fn target(src: &Path, dest: &Path) -> Result<PathBuf> {
    Ok(if dest.is_dir() {
        dest.join(src.file_name().context("Missing file name")?)
    } else {
        dest.to_path_buf()
    })
}
fn bytes_under(path: &Path) -> Result<u64> {
    regular(path)?;
    if path.is_file() {
        return Ok(fs::metadata(path)?.len());
    }
    let mut n = 0;
    for e in fs::read_dir(path)? {
        n += bytes_under(&e?.path())?;
    }
    Ok(n)
}
fn lines(input: &[String]) -> Result<(usize, Vec<String>)> {
    let mut count = 10;
    let mut paths = Vec::new();
    let mut i = 0;
    while i < input.len() {
        if input[i] == "-n" {
            i += 1;
            count = input.get(i).context("Missing line count")?.parse()?;
        } else {
            ensure!(
                !input[i].starts_with('-'),
                "Unsupported option: {}",
                input[i]
            );
            paths.push(input[i].clone());
        }
        i += 1;
    }
    Ok((count, paths))
}
fn export_batch(input: &[String], codepage: u32) -> Result<String> {
    ensure!(!input.is_empty(), "Expected NAME=value assignments");
    let mut batch = String::from("@echo off\r\n@chcp 65001 >nul\r\n");
    for assignment in input {
        let (key, value) = assignment
            .split_once('=')
            .context("Only export NAME=value is supported")?;
        ensure!(
            !key.is_empty()
                && key.chars().enumerate().all(|(i, c)| c == '_'
                    || c.is_ascii_alphabetic()
                    || (i > 0 && c.is_ascii_digit())),
            "Invalid environment variable name"
        );
        ensure!(
            !value.contains(['\"', '!', '\r', '\n']),
            "Assignment requires native set because its quoting cannot be preserved safely"
        );
        batch.push_str(&format!("@set \"{key}={}\"\r\n", value.replace('%', "%%")));
    }
    batch.push_str(&format!("@chcp {codepage} >nul\r\n"));
    Ok(batch)
}
fn native_program(name: &str) -> PathBuf {
    #[cfg(windows)]
    {
        let root =
            PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
                .join("System32");
        if name == "powershell.exe" {
            root.join("WindowsPowerShell/v1.0/powershell.exe")
        } else {
            root.join(name)
        }
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(name)
    }
}
fn follow_tail(path: &str, n: usize) -> Result<i32> {
    use std::io::{Seek, SeekFrom};
    let mut file = fs::File::open(path)?;
    let mut reader = io::BufReader::new(&mut file);
    let mut queue = std::collections::VecDeque::new();
    loop {
        let mut line = Vec::new();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if n > 0 {
            queue.push_back(line);
            if queue.len() > n {
                queue.pop_front();
            }
        }
    }
    drop(reader);
    let mut out = io::stdout();
    for line in queue {
        out.write_all(&line)?;
    }
    out.flush()?;
    loop {
        if file.metadata()?.len() < file.stream_position()? {
            file.seek(SeekFrom::Start(0))?;
        }
        let mut bytes = [0; 8192];
        let n = file.read(&mut bytes)?;
        if n > 0 {
            out.write_all(&bytes[..n])?;
            out.flush()?;
        } else {
            std::thread::sleep(Duration::from_millis(150));
        }
    }
}
/// Used only by a helper process in the existing shell. Never starts a new
/// persistent shell, changes the caller's privileges, or executes argument code.
pub fn execute(name: &str, input: &[String]) -> Result<i32> {
    match name {
        "pwd" => {
            ensure!(input.is_empty(), "pwd takes no options");
            println!("{}", std::env::current_dir()?.display());
        }
        "clear" => {
            ensure!(input.is_empty(), "clear takes no options");
            print!("\x1b[2J\x1b[H");
            io::stdout().flush()?;
        }
        "ls" => {
            let a = arguments(input, &["-l", "-a", "-h"])?;
            if a.flags.contains(&"-l".into()) && a.flags.contains(&"-h".into()) {
                let paths = if a.paths.is_empty() {
                    vec![".".into()]
                } else {
                    a.paths
                };
                let all = a.flags.contains(&"-a".into());
                for path in paths {
                    no_glob(&path)?;
                    let root = Path::new(&path);
                    let mut entries: Vec<PathBuf> = if root.is_dir() {
                        fs::read_dir(root)?
                            .map(|e| e.map(|e| e.path()))
                            .collect::<std::io::Result<_>>()?
                    } else {
                        vec![root.into()]
                    };
                    entries.sort();
                    for entry in entries {
                        let name = entry.file_name().unwrap_or_default().to_string_lossy();
                        let m = fs::metadata(&entry)?;
                        #[cfg(windows)]
                        let hidden = {
                            use std::os::windows::fs::MetadataExt;
                            m.file_attributes() & 2 != 0
                        };
                        #[cfg(not(windows))]
                        let hidden = name.starts_with('.');
                        if hidden && !all {
                            continue;
                        }
                        let size = m.len();
                        let (n, unit) = if size >= 1024 * 1024 * 1024 {
                            (size as f64 / (1024.0 * 1024.0 * 1024.0), "GiB")
                        } else if size >= 1024 * 1024 {
                            (size as f64 / (1024.0 * 1024.0), "MiB")
                        } else if size >= 1024 {
                            (size as f64 / 1024.0, "KiB")
                        } else {
                            (size as f64, "B")
                        };
                        println!(
                            "{} {:>8.1} {:<3} {}",
                            if m.is_dir() { "d" } else { "-" },
                            n,
                            unit,
                            name
                        );
                    }
                }
                return Ok(0);
            }
            let mut args = vec!["/D".into(), "/C".into(), "dir".into()];
            if a.flags.contains(&"-a".into()) {
                args.push("/A".into());
            }
            if !a.flags.contains(&"-l".into()) {
                args.push("/B".into());
            }
            for p in &a.paths {
                ensure!(
                    !p.contains(['"', '&', '|', '<', '>', '^', '%', '!', '\r', '\n']),
                    "Path requires the native dir command"
                );
            }
            args.extend(a.paths);
            return status("cmd.exe", &args);
        }
        "cat" => {
            let a = arguments(input, &[])?;
            let mut out = io::stdout().lock();
            if a.paths.is_empty() {
                io::copy(&mut io::stdin().lock(), &mut out)?;
            } else {
                for p in a.paths {
                    no_glob(&p)?;
                    io::copy(&mut fs::File::open(p)?, &mut out)?;
                }
            }
        }
        "cp" | "mv" => {
            let a = arguments(input, if name == "cp" { &["-r", "-R"] } else { &[] })?;
            ensure!(a.paths.len() >= 2, "Expected source and destination");
            for p in &a.paths {
                no_glob(p)?;
            }
            let dest = Path::new(a.paths.last().unwrap());
            ensure!(
                a.paths.len() == 2 || dest.is_dir(),
                "Multiple sources require a directory destination"
            );
            let mut plan = Vec::new();
            for s in &a.paths[..a.paths.len() - 1] {
                let src = Path::new(s);
                validate_tree(src)?;
                let dst = target(src, dest)?;
                if dst.exists() {
                    validate_tree(&dst)?;
                    if src.is_file() && dst.is_file() {
                        ensure!(
                            !same_file(src, &dst)?,
                            "Source and destination refer to the same file"
                        );
                    }
                }
                let abs_src = fs::canonicalize(src)?;
                let abs_dst = if dst.exists() {
                    fs::canonicalize(&dst)?
                } else {
                    fs::canonicalize(
                        dst.parent()
                            .filter(|p| !p.as_os_str().is_empty())
                            .unwrap_or(Path::new(".")),
                    )?
                    .join(dst.file_name().context("Missing destination")?)
                };
                ensure!(
                    abs_dst != abs_src && !abs_dst.starts_with(&abs_src),
                    "Source and destination overlap"
                );
                ensure!(
                    !src.is_dir() || name == "mv" || !a.flags.is_empty(),
                    "Directory requires -r"
                );
                plan.push((src.to_path_buf(), dst));
            }
            for (src, dst) in plan {
                if name == "cp" {
                    copy_tree(&src, &dst, !a.flags.is_empty())?;
                } else {
                    fs::rename(src, dst)?;
                }
            }
        }
        "rm" => {
            let a = arguments(input, &["-r", "-R", "-f"])?;
            ensure!(!a.paths.is_empty(), "Expected file paths");
            let recursive = a.flags.iter().any(|s| s == "-r" || s == "-R");
            let force = a.flags.contains(&"-f".into());
            let mut plan = Vec::new();
            for p in &a.paths {
                no_glob(p)?;
                let path = PathBuf::from(p);
                if !path.exists() {
                    ensure!(force, "Path does not exist: {p}");
                    continue;
                }
                ensure!(
                    ![".", ".."].contains(&p.as_str()),
                    "Refusing current or parent directory"
                );
                let abs = fs::canonicalize(&path)?;
                ensure!(abs.parent().is_some(), "Refusing filesystem root");
                let cwd = fs::canonicalize(std::env::current_dir()?)?;
                ensure!(
                    !cwd.starts_with(&abs),
                    "Refusing current directory or its ancestor"
                );
                validate_tree(&path)?;
                ensure!(!path.is_dir() || recursive, "Directory requires -r");
                plan.push(path);
            }
            for path in plan {
                if path.is_dir() {
                    fs::remove_dir_all(path)?;
                } else {
                    fs::remove_file(path)?;
                }
            }
        }
        "touch" => {
            let a = arguments(input, &[])?;
            ensure!(!a.paths.is_empty(), "Expected file paths");
            for p in &a.paths {
                no_glob(p)?;
                if Path::new(p).exists() {
                    regular(Path::new(p))?;
                }
            }
            for p in a.paths {
                let f = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(p)?;
                f.set_times(
                    fs::FileTimes::new()
                        .set_accessed(SystemTime::now())
                        .set_modified(SystemTime::now()),
                )?;
            }
        }
        "head" | "tail" => {
            let follow = name == "tail" && input.iter().any(|s| s == "-f");
            let rest: Vec<_> = input
                .iter()
                .filter(|s| s.as_str() != "-f")
                .cloned()
                .collect();
            let (n, paths) = lines(&rest)?;
            ensure!(!follow || paths.len() == 1, "tail -f requires one file");
            if follow {
                return follow_tail(&paths[0], n);
            }
            let mut file = paths_or_stdin(&paths)?;
            if name == "head" {
                let mut reader = io::BufReader::new(file);
                let mut out = io::stdout().lock();
                for _ in 0..n {
                    let mut b = Vec::new();
                    if reader.read_until(b'\n', &mut b)? == 0 {
                        break;
                    }
                    out.write_all(&b)?;
                }
            } else {
                let mut reader = io::BufReader::new(&mut file);
                let mut queue = std::collections::VecDeque::new();
                loop {
                    let mut b = Vec::new();
                    if reader.read_until(b'\n', &mut b)? == 0 {
                        break;
                    }
                    if n > 0 {
                        queue.push_back(b);
                        if queue.len() > n {
                            queue.pop_front();
                        }
                    }
                }
                let mut out = io::stdout();
                for b in queue {
                    out.write_all(&b)?;
                }
                out.flush()?;
            }
        }
        "grep" => {
            let a = arguments(input, &["-i", "-n", "-F"])?;
            ensure!(!a.paths.is_empty(), "Expected search pattern");
            let pattern = &a.paths[0];
            ensure!(
                !pattern.contains(['\r', '\n']),
                "Unsupported multiline pattern"
            );
            ensure!(
                a.flags.contains(&"-F".into())
                    || !pattern.contains([
                        '[', ']', '(', ')', '*', '+', '?', '.', '^', '$', '\\', '{', '}', '|'
                    ]),
                "Regex form is not equivalent to findstr; use -F or native findstr"
            );
            let insensitive = a.flags.contains(&"-i".into());
            let numbered = a.flags.contains(&"-n".into());
            let mut found = false;
            let files = &a.paths[1..];
            let input_files: Vec<Option<&str>> = if files.is_empty() {
                vec![None]
            } else {
                files.iter().map(|s| Some(s.as_str())).collect()
            };
            for p in input_files {
                let reader: Box<dyn Read> = if let Some(p) = p {
                    Box::new(fs::File::open(p)?)
                } else {
                    Box::new(io::stdin())
                };
                for (i, line) in io::BufReader::new(reader).lines().enumerate() {
                    let line = line?;
                    let matches = if insensitive {
                        line.to_lowercase().contains(&pattern.to_lowercase())
                    } else {
                        line.contains(pattern)
                    };
                    if matches {
                        found = true;
                        if files.len() > 1 {
                            print!("{}:", p.unwrap());
                        }
                        if numbered {
                            print!("{}:", i + 1);
                        }
                        println!("{line}");
                    }
                }
            }
            return Ok(if found { 0 } else { 1 });
        }
        "which" => return status("where.exe", input),
        "ps" => {
            ensure!(
                input.is_empty() || input == ["aux"],
                "Only ps / ps aux supported"
            );
            return status("tasklist.exe", &[]);
        }
        "kill" => {
            ensure!(
                input.len() == 1,
                "Only kill PID supported; force is not implicit"
            );
            let pid: u32 = input[0].parse()?;
            ensure!(pid > 0, "Invalid PID");
            return status("taskkill.exe", &["/PID".into(), pid.to_string()]);
        }
        "env" | "printenv" => {
            ensure!(
                input.len() <= 1,
                "Only environment listing or one variable supported"
            );
            if let Some(key) = input.first() {
                if let Some(v) = std::env::var_os(key) {
                    println!("{}", v.to_string_lossy());
                } else {
                    return Ok(1);
                }
            } else {
                let mut vars: Vec<_> = std::env::vars_os().collect();
                vars.sort();
                for (k, v) in vars {
                    println!("{}={}", k.to_string_lossy(), v.to_string_lossy());
                }
            }
        }
        "uname" => {
            let a = arguments(input, &["-a", "-s", "-r", "-m"])?;
            ensure!(a.paths.is_empty(), "Unexpected operand");
            let mut fields = Vec::new();
            if a.flags.is_empty() || a.flags.iter().any(|f| f == "-s" || f == "-a") {
                fields.push("Windows".to_string());
            }
            if a.flags.iter().any(|f| f == "-r" || f == "-a") {
                let output = Command::new(native_program("cmd.exe"))
                    .args(["/D", "/C", "ver"])
                    .output()?;
                ensure!(output.status.success(), "Windows version query failed");
                fields.push(String::from_utf8_lossy(&output.stdout).trim().to_string());
            }
            if a.flags.iter().any(|f| f == "-m" || f == "-a") {
                fields.push(std::env::consts::ARCH.to_string());
            }
            println!("{}", fields.join(" "));
        }
        "history" => {
            ensure!(input.is_empty(), "history takes no options");
            return status("doskey.exe", &["/history".into()]);
        }
        "man" => {
            ensure!(input.len() <= 1, "Expected zero or one command");
            for p in input {
                ensure!(
                    p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                    "Unsupported help name"
                );
            }
            let mut a = vec!["/D".into(), "/C".into(), "help".into()];
            a.extend_from_slice(input);
            return status("cmd.exe", &a);
        }
        "sleep" => {
            let seconds: f64 = one(input)?.parse()?;
            let duration = Duration::try_from_secs_f64(seconds)
                .context("Sleep requires a finite nonnegative duration")?;
            std::thread::sleep(duration);
        }
        "du" => {
            let a = arguments(input, &["-s", "-h"])?;
            let paths = if a.paths.is_empty() {
                vec![".".into()]
            } else {
                a.paths
            };
            for p in paths {
                let n = bytes_under(Path::new(&p))?;
                if a.flags.contains(&"-h".into()) {
                    println!("{:.1} KiB\t{p}", n as f64 / 1024.0);
                } else {
                    println!("{}\t{p}", n.div_ceil(1024));
                }
            }
        }
        "df" => {
            ensure!(
                input.is_empty() || input == ["-h"],
                "Only df / df -h supported"
            );
            return status("powershell.exe",&["-NoProfile".into(),"-Command".into(),"Get-CimInstance Win32_LogicalDisk | Select-Object DeviceID,Size,FreeSpace | Format-Table -AutoSize".into()]);
        }
        "wc" => {
            let a = arguments(input, &["-l", "-w", "-m", "-c"])?;
            let mut data = Vec::new();
            paths_or_stdin(&a.paths)?.read_to_end(&mut data)?;
            let text = if a.flags.is_empty() || a.flags.iter().any(|f| f == "-w" || f == "-m") {
                std::str::from_utf8(&data)?
            } else {
                ""
            };
            let values = [
                ("-l", data.iter().filter(|&&b| b == b'\n').count()),
                ("-w", text.split_whitespace().count()),
                ("-c", data.len()),
                ("-m", text.chars().count()),
            ];
            for (flag, n) in values {
                if (a.flags.is_empty() && flag != "-m") || a.flags.contains(&flag.into()) {
                    print!("{n} ");
                }
            }
            println!("{}", a.paths.first().map(String::as_str).unwrap_or(""));
        }
        "basename" | "dirname" | "realpath" => {
            let p = Path::new(one(input)?);
            if name == "realpath" {
                println!("{}", fs::canonicalize(p)?.display());
            } else if name == "basename" {
                println!(
                    "{}",
                    p.file_name()
                        .context("No final path component")?
                        .to_string_lossy()
                );
            } else {
                println!(
                    "{}",
                    p.parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .unwrap_or(Path::new("."))
                        .display()
                );
            }
        }
        "open" => {
            ensure!(
                std::env::var_os("CLASSROOM_TERMINAL_ALLOW_OPEN").is_some(),
                "Opening desktop applications is disabled by managed terminal policy"
            );
            let value = one(input)?;
            #[cfg(windows)]
            {
                use windows::{
                    core::{w, PCWSTR},
                    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
                };
                ensure!(!value.contains(['\r', '\n']), "Invalid open target");
                let target: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
                // SAFETY: buffers remain alive; this explicitly policy-authorized action
                // opens the literal target, with no command interpreter or runas verb.
                let result = unsafe {
                    ShellExecuteW(
                        None,
                        w!("open"),
                        PCWSTR(target.as_ptr()),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        SW_SHOWNORMAL,
                    )
                };
                ensure!(
                    result.0 as usize > 32,
                    "Default application could not open the target"
                );
            }
            #[cfg(not(windows))]
            {
                let _ = value;
                bail!("Compatibility open is Windows-only");
            }
        }
        "export" => {
            ensure!(!input.is_empty(), "Expected NAME=value assignments");
            let path = std::env::var_os("CLASSROOM_TERMINAL_EXPORT")
                .context("Export must run within a compatible managed CMD session")?;
            #[cfg(windows)]
            let codepage = unsafe { windows::Win32::System::Console::GetConsoleCP() };
            #[cfg(not(windows))]
            let codepage = 65001;
            let batch = export_batch(input, if codepage == 0 { 65001 } else { codepage })?;
            fs::write(path, batch)?;
        }
        _ => bail!("Unsupported compatibility command"),
    }
    Ok(0)
}

pub fn early_operation() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("--terminal-unix") {
        return None;
    }
    #[cfg(windows)]
    {
        use windows::Win32::System::Console::{AttachConsole, GetConsoleCP, ATTACH_PARENT_PROCESS};
        // SAFETY: this short-lived literal-command helper attaches only its
        // parent CMD's existing console. It never allocates a visible window.
        unsafe {
            if GetConsoleCP() == 0 {
                let _ = AttachConsole(ATTACH_PARENT_PROCESS);
            }
            let _ = windows::Win32::System::Console::SetConsoleCtrlHandler(None, false);
        }
    }
    Some(
        match args
            .get(2)
            .context("Missing compatibility command")
            .and_then(|name| execute(name, &args[3..]))
        {
            Ok(code) => code,
            Err(e) => {
                eprintln!("{e:#}");
                2
            }
        },
    )
}

#[cfg(windows)]
pub struct Helpers(pub PathBuf);
#[cfg(windows)]
impl Helpers {
    pub fn install(root: &Path) -> Result<Self> {
        let dir = root.join(format!("compat-{}", crate::classroom_tools::nonce()?));
        fs::create_dir(&dir)?;
        let exe = std::env::current_exe()?;
        let exe = exe
            .to_str()
            .context("Helper executable path must be Unicode")?;
        ensure!(
            !exe.contains(['%', '!', '"', '\r', '\n']),
            "Helper executable path cannot be represented safely in CMD"
        );
        for name in COMMANDS {
            let body = if *name == "export" {
                format!("@\"{exe}\" --terminal-unix export %*\r\n@if errorlevel 1 exit /b %errorlevel%\r\n@call \"%CLASSROOM_TERMINAL_EXPORT%\"\r\n@del /q \"%CLASSROOM_TERMINAL_EXPORT%\" >nul 2>&1\r\n@exit /b 0\r\n")
            } else {
                format!("@\"{exe}\" --terminal-unix {name} %*\r\n@exit /b %errorlevel%\r\n")
            };
            fs::write(dir.join(format!("{name}.cmd")), body)?;
        }
        Ok(Self(dir))
    }
}
#[cfg(windows)]
impl Drop for Helpers {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flags_are_parsed_only_before_double_dash() {
        let a = arguments(&["-rf".into(), "--".into(), "-name".into()], &["-r", "-f"]).unwrap();
        assert_eq!(a.flags, ["-r", "-f"]);
        assert_eq!(a.paths, ["-name"]);
    }
    #[test]
    fn unknown_options_rejected() {
        assert!(arguments(&["--recursive-all".into()], &["-r"]).is_err());
    }
    #[test]
    fn paths_are_not_interpreted_as_commands() {
        let a = arguments(&["a b/đề & file.txt".into()], &[]).unwrap();
        assert_eq!(a.paths, ["a b/đề & file.txt"]);
    }
    #[test]
    fn destructive_wildcards_fail_closed() {
        assert!(no_glob("*.txt").is_err());
        assert!(no_glob("report?.txt").is_err());
        assert!(no_glob("đề bài.txt").is_ok());
    }
    #[test]
    fn line_count_validated() {
        assert_eq!(
            lines(&["-n".into(), "3".into(), "a b.txt".into()]).unwrap(),
            (3, vec!["a b.txt".into()])
        );
        assert!(lines(&["-n".into(), "-1".into()]).is_err());
    }
    #[test]
    fn native_names_not_installed() {
        for name in [
            "cd", "mkdir", "echo", "ping", "set", "date", "rmdir", "whoami",
        ] {
            assert!(!COMMANDS.contains(&name));
        }
    }
    #[test]
    fn root_delete_rejected() {
        assert!(execute("rm", &["-rf".into(), "/".into()]).is_err());
        assert!(execute("rm", &["-rf".into(), ".".into()]).is_err());
    }
    #[test]
    fn invalid_sleep_rejected() {
        for n in ["-1", "NaN", "inf", "1e100"] {
            assert!(execute("sleep", &[n.into()]).is_err());
        }
    }
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "classroom-compat-{}",
                crate::classroom_tools::nonce().unwrap()
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn file_operations_preserve_unicode_and_spaces() {
        let f = Fixture::new();
        let source = f.0.join("đề bài.txt");
        let copy = f.0.join("bản sao.txt");
        let moved = f.0.join("đã đổi.txt");
        let text = "xin chào\nsecond line\nlast\n";
        fs::write(&source, text).unwrap();
        let paths = |a: &Path, b: &Path| {
            vec![
                a.to_string_lossy().into_owned(),
                b.to_string_lossy().into_owned(),
            ]
        };
        assert_eq!(execute("cp", &paths(&source, &copy)).unwrap(), 0);
        assert_eq!(fs::read_to_string(&copy).unwrap(), text);
        assert_eq!(execute("mv", &paths(&copy, &moved)).unwrap(), 0);
        assert!(!copy.exists());
        assert_eq!(fs::read_to_string(&moved).unwrap(), text);
        assert_eq!(
            execute("touch", &[source.to_string_lossy().into_owned()]).unwrap(),
            0
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), text);
        assert_eq!(
            execute("rm", &[moved.to_string_lossy().into_owned()]).unwrap(),
            0
        );
        assert!(!moved.exists());
        assert!(source.exists());
    }
    #[test]
    fn recursive_copy_and_delete_keep_scope() {
        let f = Fixture::new();
        let src = f.0.join("src");
        let dest = f.0.join("dest");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("one.txt"), "one").unwrap();
        let args = vec![
            "-r".into(),
            src.to_string_lossy().into_owned(),
            dest.to_string_lossy().into_owned(),
        ];
        assert_eq!(execute("cp", &args).unwrap(), 0);
        assert_eq!(fs::read(dest.join("one.txt")).unwrap(), b"one");
        assert!(execute("rm", &[dest.to_string_lossy().into_owned()]).is_err());
        assert!(dest.exists());
        assert_eq!(
            execute("rm", &["-rf".into(), dest.to_string_lossy().into_owned()]).unwrap(),
            0
        );
        assert!(!dest.exists());
        assert!(src.join("one.txt").exists());
    }
    #[test]
    fn unsafe_copy_overlap_is_rejected_before_writing() {
        let f = Fixture::new();
        let src = f.0.join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("one"), b"keep").unwrap();
        assert!(execute(
            "cp",
            &[
                "-r".into(),
                src.to_string_lossy().into_owned(),
                src.join("child").to_string_lossy().into_owned()
            ]
        )
        .is_err());
        assert!(!src.join("child").exists());
    }
    #[test]
    fn readers_and_counts_accept_unicode_file() {
        let f = Fixture::new();
        let path = f.0.join("with space.txt");
        fs::write(&path, "hello 世界\nlast\n").unwrap();
        let p = path.to_string_lossy().into_owned();
        for name in [
            "cat", "head", "tail", "wc", "du", "basename", "dirname", "realpath",
        ] {
            assert_eq!(
                execute(name, std::slice::from_ref(&p)).unwrap(),
                0,
                "{name}"
            );
        }
        assert_eq!(
            execute("head", &["-n".into(), "1".into(), p.clone()]).unwrap(),
            0
        );
        assert_eq!(
            execute("tail", &["-n".into(), "1".into(), p.clone()]).unwrap(),
            0
        );
        assert_eq!(
            execute("grep", &["-in".into(), "HELLO".into(), p.clone()]).unwrap(),
            0
        );
        assert_eq!(execute("grep", &["absent".into(), p]).unwrap(), 1);
    }
    #[test]
    fn unsupported_forms_do_not_run_shell_text() {
        for (name, args) in [
            ("grep", vec!["a.*b"]),
            ("kill", vec!["123 & echo BAD"]),
            ("sleep", vec!["1 & echo BAD"]),
            ("cp", vec!["-f", "a", "b"]),
            ("rm", vec!["--no-preserve-root", "/"]),
            ("head", vec!["-x"]),
        ] {
            assert!(execute(name, &strings(&args)).is_err(), "{name}");
        }
    }
    #[test]
    fn native_commands_are_not_compatibility_entry_points() {
        for name in [
            "cd", "mkdir", "echo", "ping", "set", "date", "rmdir", "whoami",
        ] {
            assert!(execute(name, &[]).is_err());
        }
    }
    #[cfg(unix)]
    #[test]
    fn destructive_operations_do_not_follow_links() {
        let f = Fixture::new();
        let real = f.0.join("real");
        fs::create_dir(&real).unwrap();
        fs::write(real.join("keep"), b"keep").unwrap();
        let link = f.0.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(execute("rm", &["-rf".into(), link.to_string_lossy().into_owned()]).is_err());
        assert!(real.join("keep").exists());
    }
    #[test]
    fn export_is_literal_and_restores_codepage() {
        let batch = export_batch(&["NAME=đề bài & literal %PATH%".into()], 437).unwrap();
        assert!(batch.starts_with("@echo off\r\n@chcp 65001"));
        assert!(batch.contains("@set \"NAME=đề bài & literal %%PATH%%\""));
        assert!(batch.ends_with("@chcp 437 >nul\r\n"));
        for value in [
            "BAD-NAME=x",
            "X=x\" & del everything",
            "X=x\nset BAD=1",
            "X=x!evil!",
        ] {
            assert!(export_batch(&[value.into()], 437).is_err());
        }
    }
    #[test]
    fn copy_rejects_hard_link_alias_without_truncation() {
        let f = Fixture::new();
        let a = f.0.join("original");
        let b = f.0.join("alias");
        fs::write(&a, b"must survive").unwrap();
        fs::hard_link(&a, &b).unwrap();
        assert!(execute(
            "cp",
            &[
                a.to_string_lossy().into_owned(),
                b.to_string_lossy().into_owned()
            ]
        )
        .is_err());
        assert_eq!(fs::read(&a).unwrap(), b"must survive");
    }
}
