//! Directory listing, filtered tree planning and the checksummed tree stream
//! shared by file collection (Student → Teacher) and the two-panel file
//! manager (both directions). Pure file/socket work: no media/control thread
//! ever calls into this module.
//!
//! Stream (sender → receiver), every message a length-prefixed `ToolPacket`:
//! `begin` → receiver `ok`|`error`; then per item `entry` (`file`|`dir`|`skip`),
//! raw bytes + `checksum` for files, receiver `stored`|`skipped`|`error`;
//! finally `end` → `ok`. Either side may abort by closing the connection.
use super::{
    configure, numbered_name, packet, read, safe_name, same_content, send, user_folder, CHUNK,
    MAX_FILE,
};
use anyhow::{Context, Result};
use protocol::ToolPacket;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

pub const MAX_LIST: usize = 5000;
pub const MAX_ITEMS: usize = 50_000;
const MAX_DEPTH: usize = 32;
const MAX_NOTES: usize = 20;
/// ~4 MiB/s per stream: bulk collection from many Students at once.
pub const BULK_DELAY: Duration = Duration::from_millis(16);
/// ~16 MiB/s: interactive single-Student file manager.
pub const INTERACTIVE_DELAY: Duration = Duration::from_millis(4);
const STREAM_TIMEOUT: Duration = Duration::from_secs(120);

/// Which files a walk accepts. Limits are clamped by the sender regardless of
/// what the Teacher requested.
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Filter {
    pub exts: Vec<String>,
    pub recursive: bool,
    pub hidden: bool,
    pub max_file_mb: u64,
    pub max_total_mb: u64,
}
impl Default for Filter {
    fn default() -> Self {
        Self {
            exts: Vec::new(),
            recursive: true,
            hidden: false,
            max_file_mb: 200,
            max_total_mb: 4096,
        }
    }
}
impl Filter {
    fn max_file(&self) -> u64 {
        self.max_file_mb.clamp(1, 4096) * 1024 * 1024
    }
    fn max_total(&self) -> u64 {
        self.max_total_mb.clamp(1, 32 * 1024) * 1024 * 1024
    }
    fn accepts(&self, name: &str) -> bool {
        if self.exts.is_empty() {
            return true;
        }
        let Some((_, ext)) = name.rsplit_once('.') else {
            return false;
        };
        self.exts.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }
}
/// `"docx, .PDF; *.py"` → `["docx","pdf","py"]`.
pub fn parse_exts(text: &str) -> Vec<String> {
    let mut exts: Vec<String> = text
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(|e| e.trim().trim_start_matches('*').trim_start_matches('.'))
        .filter(|e| !e.is_empty() && e.len() <= 16)
        .map(str::to_ascii_lowercase)
        .collect();
    exts.sort();
    exts.dedup();
    exts.truncate(32);
    exts
}

/// Describes an I/O failure; macOS privacy denials get an actionable hint.
pub fn describe(error: &anyhow::Error) -> String {
    let denied = error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied)
    });
    if denied && cfg!(target_os = "macos") {
        "Access denied by macOS. Allow Classroom in System Settings → Privacy & Security → Files and Folders (or Full Disk Access), then try again".into()
    } else if denied {
        format!("Access denied: {error:#}")
    } else {
        format!("{error:#}")
    }
}

/// `""`/`~desktop`/`~documents`/`~downloads`/`~home` or an absolute path.
pub fn resolve(path: &str) -> Result<PathBuf> {
    match path.trim().to_ascii_lowercase().as_str() {
        "" | "~desktop" => user_folder("Desktop"),
        "~documents" => user_folder("Documents"),
        "~downloads" => user_folder("Downloads"),
        "~home" => user_folder(""),
        _ => {
            let path = PathBuf::from(path.trim());
            anyhow::ensure!(path.is_absolute(), "Path must be absolute");
            Ok(path)
        }
    }
}

fn is_hidden(name: &str, meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x6 != 0 {
            return true;
        }
    }
    let _ = meta;
    name.starts_with('.')
}
fn junk(name: &str) -> bool {
    name.starts_with("~$")
        || ["desktop.ini", "thumbs.db", ".ds_store"]
            .iter()
            .any(|j| name.eq_ignore_ascii_case(j))
}
fn modified(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// Quick-access places shown above every browser panel.
fn roots() -> Vec<Value> {
    let mut roots: Vec<Value> = Vec::new();
    for (name, key) in [
        ("Desktop", "Desktop"),
        ("Documents", "Documents"),
        ("Downloads", "Downloads"),
        ("Home", ""),
    ] {
        if let Ok(path) = user_folder(key) {
            roots.push(json!({"name":name,"path":path.to_string_lossy()}));
        }
    }
    #[cfg(windows)]
    for letter in b'C'..=b'Z' {
        let drive = format!("{}:\\", char::from(letter));
        if Path::new(&drive).exists() {
            roots.push(json!({"name":format!("{}:", char::from(letter)),"path":drive}));
        }
    }
    #[cfg(not(windows))]
    roots.push(json!({"name":"Computer","path":"/"}));
    roots
}

/// Lists one directory as JSON: `{path, sep, os, parent, roots, entries,
/// truncated}` or `{error}`. Entries are `{n,d,s,m,h}` (name, is-dir, size,
/// modified unix seconds, hidden); sorting is the caller's choice.
pub fn list(path: &str, hidden: bool) -> Value {
    match list_inner(path, hidden) {
        Ok(value) => value,
        Err(error) => json!({"error":describe(&error),"path":path}),
    }
}
fn list_inner(path: &str, hidden: bool) -> Result<Value> {
    let dir = resolve(path)?;
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry in fs::read_dir(&dir).with_context(|| format!("Cannot open {}", dir.display()))? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(mut meta) = entry.metadata() else {
            continue;
        };
        let hidden_entry = is_hidden(&name, &meta);
        if hidden_entry && !hidden {
            continue;
        }
        if meta.file_type().is_symlink() {
            // Show where a link leads without ever recursing through it.
            if let Ok(target) = fs::metadata(entry.path()) {
                meta = target;
            }
        }
        if entries.len() >= MAX_LIST {
            truncated = true;
            break;
        }
        entries.push(json!({"n":name,"d":meta.is_dir(),"s":if meta.is_dir() {0} else {meta.len()},"m":modified(&meta),"h":hidden_entry}));
    }
    Ok(json!({
        "path":dir.to_string_lossy(),
        "sep":std::path::MAIN_SEPARATOR.to_string(),
        "os":std::env::consts::OS,
        "parent":dir.parent().map_or(String::new(), |p| p.to_string_lossy().into_owned()),
        "roots":roots(),
        "entries":entries,
        "truncated":truncated,
    }))
}

pub struct Item {
    pub path: PathBuf,
    /// Receiver-relative path, `/`-separated.
    pub rel: String,
    pub dir: bool,
}
#[derive(Default)]
pub struct Plan {
    pub items: Vec<Item>,
    pub files: u64,
    pub bytes: u64,
    pub skipped: u64,
    pub notes: Vec<String>,
}
impl Plan {
    fn note(&mut self, text: String) {
        if self.notes.len() < MAX_NOTES {
            self.notes.push(text);
        }
    }
    fn skip(&mut self, text: String) {
        self.skipped += 1;
        self.note(text);
    }
    fn add_file(&mut self, path: PathBuf, rel: String, size: u64, filter: &Filter) -> bool {
        if self.items.len() >= MAX_ITEMS {
            self.skip(format!("More than {MAX_ITEMS} items; the rest was skipped"));
            return false;
        }
        if size > filter.max_file() {
            self.skip(format!("{rel}: larger than {} MB", filter.max_file_mb));
            return true;
        }
        if self.bytes + size > filter.max_total() {
            self.skip(format!(
                "{rel}: total size limit of {} MB reached",
                filter.max_total_mb
            ));
            return true;
        }
        self.bytes += size;
        self.files += 1;
        self.items.push(Item {
            path,
            rel,
            dir: false,
        });
        true
    }
}
/// Walks `roots` into a transfer plan. Directories are walked (and, with
/// `keep_root`, recreated under their own name); files named explicitly in
/// `roots` ignore the extension filter. Symbolic links are never followed.
pub fn plan(
    roots: &[PathBuf],
    filter: &Filter,
    keep_root: bool,
    cancel: &AtomicBool,
) -> Result<Plan> {
    let mut plan = Plan::default();
    for root in roots {
        anyhow::ensure!(!cancel.load(Ordering::Acquire), "Cancelled");
        let meta = match fs::symlink_metadata(root) {
            Ok(m) => m,
            Err(e) if roots.len() == 1 => {
                return Err(e).with_context(|| format!("Cannot open {}", root.display()))
            }
            Err(e) => {
                plan.skip(format!("{}: {e}", root.display()));
                continue;
            }
        };
        let base = root
            .file_name()
            .map_or_else(
                || root.to_string_lossy().replace([':', '\\', '/'], ""),
                |n| n.to_string_lossy().into_owned(),
            )
            .trim()
            .to_owned();
        if meta.is_file() {
            plan.add_file(
                root.clone(),
                base,
                meta.len(),
                &Filter {
                    exts: Vec::new(),
                    ..filter.clone()
                },
            );
        } else if meta.is_dir() {
            let prefix = if keep_root { base } else { String::new() };
            if keep_root && filter.exts.is_empty() {
                plan.items.push(Item {
                    path: root.clone(),
                    rel: prefix.clone(),
                    dir: true,
                });
            }
            walk(root, prefix, roots.len() == 1, filter, cancel, &mut plan)?;
        } else {
            plan.skip(format!("{}: not a regular file or folder", root.display()));
        }
    }
    Ok(plan)
}
fn walk(
    root: &Path,
    prefix: String,
    only: bool,
    filter: &Filter,
    cancel: &AtomicBool,
    plan: &mut Plan,
) -> Result<()> {
    let mut stack = vec![(root.to_path_buf(), prefix, 0usize)];
    while let Some((dir, prefix, depth)) = stack.pop() {
        anyhow::ensure!(!cancel.load(Ordering::Acquire), "Cancelled");
        let reader = match fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) if depth == 0 && only => {
                return Err(e).with_context(|| format!("Cannot open {}", dir.display()))
            }
            Err(e) => {
                plan.note(format!("{}: {e}", dir.display()));
                continue;
            }
        };
        let mut children: Vec<_> = reader.filter_map(std::result::Result::ok).collect();
        children.sort_by_key(fs::DirEntry::file_name);
        for entry in children {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.file_type().is_symlink()
                || junk(&name)
                || (!filter.hidden && is_hidden(&name, &meta))
            {
                continue;
            }
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if meta.is_dir() {
                if filter.recursive && depth < MAX_DEPTH {
                    if filter.exts.is_empty() && plan.items.len() < MAX_ITEMS {
                        plan.items.push(Item {
                            path: entry.path(),
                            rel: rel.clone(),
                            dir: true,
                        });
                    }
                    stack.push((entry.path(), rel, depth + 1));
                }
            } else if meta.is_file()
                && filter.accepts(&name)
                && !plan.add_file(entry.path(), rel, meta.len(), filter)
            {
                return Ok(());
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub files_total: u64,
    pub files_done: u64,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub skipped: u64,
    pub failed: u64,
    pub current: String,
    pub notes: Vec<String>,
}
impl Progress {
    fn note(&mut self, text: String) {
        if self.notes.len() < MAX_NOTES {
            self.notes.push(text);
        }
    }
}

/// Raises the stream timeouts once the (short) handshake is over: the peer may
/// be hashing or publishing a large file between messages.
pub fn relax(s: &TcpStream) -> Result<()> {
    s.set_read_timeout(Some(STREAM_TIMEOUT))?;
    s.set_write_timeout(Some(STREAM_TIMEOUT))?;
    Ok(())
}

pub fn send_tree(
    s: &mut TcpStream,
    plan: &Plan,
    delay: Duration,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&Progress),
) -> Result<Progress> {
    configure(s)?;
    relax(s)?;
    let mut p = Progress {
        files_total: plan.files,
        bytes_total: plan.bytes,
        skipped: plan.skipped,
        notes: plan.notes.clone(),
        ..Progress::default()
    };
    let mut begin = packet("begin");
    begin.size = plan.bytes;
    begin.done = plan.files;
    begin.text = json!({"skipped":plan.skipped,"notes":plan.notes}).to_string();
    while begin.text.len() > 4000 {
        begin.text = json!({"skipped":plan.skipped}).to_string();
    }
    send(s, &begin)?;
    let reply = read(s)?;
    anyhow::ensure!(
        reply.action == "ok",
        "{}",
        if reply.text.is_empty() {
            "Receiver rejected the transfer".into()
        } else {
            reply.text
        }
    );
    progress(&p);
    let mut buffer = vec![0; CHUNK];
    let mut reported = Instant::now();
    for item in &plan.items {
        anyhow::ensure!(!cancel.load(Ordering::Acquire), "Cancelled");
        let mut entry = packet("entry");
        entry.name.clone_from(&item.rel);
        if item.dir {
            entry.status = "dir".into();
            send(s, &entry)?;
            continue;
        }
        p.current.clone_from(&item.rel);
        let mut file = match File::open(&item.path) {
            Ok(f) => f,
            Err(e) => {
                entry.status = "skip".into();
                entry.text = describe(&anyhow::Error::from(e));
                send(s, &entry)?;
                p.failed += 1;
                p.note(format!("{}: {}", item.rel, entry.text));
                continue;
            }
        };
        let total = file.metadata()?.len();
        anyhow::ensure!(total <= MAX_FILE, "File too large");
        entry.status = "file".into();
        entry.size = total;
        send(s, &entry)?;
        let mut hash = Sha256::new();
        let mut done = 0;
        while done < total {
            let started = Instant::now();
            anyhow::ensure!(!cancel.load(Ordering::Acquire), "Cancelled");
            let n = file.read(&mut buffer[..usize::try_from((total - done).min(CHUNK as u64))?])?;
            anyhow::ensure!(n > 0, "{} changed during transfer", item.rel);
            hash.update(&buffer[..n]);
            s.write_all(&buffer[..n])?;
            done += n as u64;
            if reported.elapsed() >= Duration::from_millis(250) {
                p.bytes_done += done;
                progress(&p);
                p.bytes_done -= done;
                reported = Instant::now();
            }
            thread::sleep(delay.saturating_sub(started.elapsed()));
        }
        let mut finish = packet("checksum");
        finish.hash = format!("{:x}", hash.finalize());
        send(s, &finish)?;
        let ack = read(s)?;
        p.bytes_done += total;
        match ack.action.as_str() {
            "stored" => p.files_done += 1,
            "skipped" => {
                p.files_done += 1;
                p.skipped += 1;
            }
            _ => {
                p.failed += 1;
                p.note(format!("{}: {}", item.rel, ack.text));
            }
        }
        progress(&p);
    }
    p.current.clear();
    send(s, &packet("end"))?;
    anyhow::ensure!(read(s)?.action == "ok", "Receiver did not finish");
    progress(&p);
    Ok(p)
}

/// Where received files go. `flat` drops the relative structure.
pub struct Sink {
    pub root: PathBuf,
    pub flat: bool,
    pub tag: String,
}
impl Sink {
    fn destination(&self, rel: &str) -> Result<(PathBuf, String)> {
        let mut parts: Vec<String> = rel
            .split('/')
            .filter(|part| !part.is_empty())
            .map(safe_name)
            .collect();
        anyhow::ensure!(
            !parts.is_empty() && parts.len() <= MAX_DEPTH + 2,
            "Invalid path"
        );
        let name = parts.pop().unwrap_or_default();
        let mut dir = self.root.clone();
        if !self.flat {
            dir.extend(parts);
        }
        Ok((dir, name))
    }
}
/// Receives a tree into `sink`. `Err` only for connection/protocol failures;
/// per-file problems are counted in the returned [`Progress`].
pub fn receive_tree(
    s: &mut TcpStream,
    sink: &Sink,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&Progress),
) -> Result<Progress> {
    configure(s)?;
    relax(s)?;
    let begin = read(s)?;
    if begin.action == "abort" {
        anyhow::bail!("{}", begin.text);
    }
    anyhow::ensure!(begin.action == "begin", "Invalid tree stream");
    if let Err(error) = fs::create_dir_all(&sink.root)
        .and_then(|()| fs::read_dir(&sink.root).map(|_| ()))
        .map_err(anyhow::Error::from)
        .context("Destination folder is not usable")
    {
        let mut reject = packet("error");
        reject.text = describe(&error);
        send(s, &reject)?;
        return Err(error);
    }
    anyhow::ensure!(
        begin.done <= MAX_ITEMS as u64 && begin.size <= 64 * 1024 * 1024 * 1024,
        "Transfer too large"
    );
    let mut p = Progress {
        files_total: begin.done,
        bytes_total: begin.size,
        ..Progress::default()
    };
    if let Ok(info) = serde_json::from_str::<Value>(&begin.text) {
        p.skipped = info["skipped"].as_u64().unwrap_or(0);
        for note in info["notes"].as_array().into_iter().flatten() {
            if let Some(note) = note.as_str() {
                p.note(note.chars().take(300).collect());
            }
        }
    }
    send(s, &packet("ok"))?;
    progress(&p);
    let part_name = format!(".classroom-{}.part", sink.tag);
    let mut buffer = vec![0; CHUNK];
    let mut reported = Instant::now();
    loop {
        anyhow::ensure!(!cancel.load(Ordering::Acquire), "Cancelled");
        let entry = read(s)?;
        match entry.action.as_str() {
            "end" => {
                send(s, &packet("ok"))?;
                p.current.clear();
                progress(&p);
                return Ok(p);
            }
            "entry" => {}
            _ => anyhow::bail!("Invalid tree entry"),
        }
        match entry.status.as_str() {
            "dir" => {
                if !sink.flat {
                    if let Ok((dir, name)) = sink.destination(&entry.name) {
                        let _ = fs::create_dir_all(dir.join(name));
                    }
                }
            }
            "skip" => {
                p.failed += 1;
                p.note(format!("{}: {}", entry.name, entry.text));
            }
            "file" => {
                anyhow::ensure!(entry.size <= MAX_FILE, "File exceeds size limit");
                p.current.clone_from(&entry.name);
                let outcome = receive_file(
                    s,
                    sink,
                    &entry,
                    &part_name,
                    cancel,
                    &mut buffer,
                    &mut |done| {
                        if reported.elapsed() >= Duration::from_millis(250) {
                            let mut snapshot = p.clone();
                            snapshot.bytes_done += done;
                            progress(&snapshot);
                            reported = Instant::now();
                        }
                    },
                )?;
                p.bytes_done += entry.size;
                let mut reply = packet(outcome.action());
                match outcome {
                    Outcome::Stored(hash) => {
                        p.files_done += 1;
                        reply.hash = hash;
                    }
                    Outcome::Identical => {
                        p.files_done += 1;
                        p.skipped += 1;
                    }
                    Outcome::Failed(reason) => {
                        p.failed += 1;
                        p.note(format!("{}: {reason}", entry.name));
                        reply.text = reason;
                    }
                }
                send(s, &reply)?;
                progress(&p);
            }
            _ => anyhow::bail!("Invalid tree entry"),
        }
    }
}
enum Outcome {
    Stored(String),
    Identical,
    Failed(String),
}
impl Outcome {
    fn action(&self) -> &'static str {
        match self {
            Self::Stored(_) => "stored",
            Self::Identical => "skipped",
            Self::Failed(_) => "error",
        }
    }
}
struct Partial(PathBuf);
impl Drop for Partial {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
/// Always consumes exactly `entry.size` bytes plus the checksum packet so one
/// unwritable file never desynchronises the stream.
fn receive_file(
    s: &mut TcpStream,
    sink: &Sink,
    entry: &ToolPacket,
    part_name: &str,
    cancel: &AtomicBool,
    buffer: &mut [u8],
    tick: &mut dyn FnMut(u64),
) -> Result<Outcome> {
    let target = sink.destination(&entry.name).and_then(|(dir, name)| {
        fs::create_dir_all(&dir)
            .with_context(|| format!("Cannot create {}", dir.display()))
            .map(|()| (dir, name))
    });
    let (target, mut failure) = match target {
        Ok(t) => (Some(t), None),
        Err(e) => (None, Some(describe(&e))),
    };
    let part = target.as_ref().map(|(dir, _)| Partial(dir.join(part_name)));
    let mut output = match &part {
        Some(part) => match File::create(&part.0) {
            Ok(f) => Some(f),
            Err(e) => {
                failure = Some(describe(&anyhow::Error::from(e)));
                None
            }
        },
        None => None,
    };
    let mut hash = Sha256::new();
    let mut done = 0;
    while done < entry.size {
        anyhow::ensure!(!cancel.load(Ordering::Acquire), "Cancelled");
        let n = s.read(&mut buffer[..usize::try_from((entry.size - done).min(CHUNK as u64))?])?;
        anyhow::ensure!(n > 0, "Transfer interrupted");
        hash.update(&buffer[..n]);
        if let Some(file) = &mut output {
            if let Err(e) = file.write_all(&buffer[..n]) {
                failure = Some(describe(&anyhow::Error::from(e)));
                output = None;
            }
        }
        done += n as u64;
        tick(done);
    }
    let checksum = read(s)?;
    let digest = format!("{:x}", hash.finalize());
    anyhow::ensure!(checksum.action == "checksum", "Missing file checksum");
    if let Some(reason) = failure {
        return Ok(Outcome::Failed(reason));
    }
    let (Some(file), Some(part), Some((dir, name))) = (output, part, target) else {
        return Ok(Outcome::Failed("File could not be written".into()));
    };
    if checksum.hash != digest {
        return Ok(Outcome::Failed("Checksum mismatch".into()));
    }
    if let Err(e) = file.sync_all() {
        return Ok(Outcome::Failed(describe(&anyhow::Error::from(e))));
    }
    drop(file);
    // create_new reserves the final name without ever replacing a file; the
    // verified part then takes its place.
    for index in 0..10_000 {
        let candidate = dir.join(numbered_name(&name, index));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(reserved) => {
                drop(reserved);
                if fs::rename(&part.0, &candidate).is_err() {
                    let copied = fs::copy(&part.0, &candidate);
                    if let Err(e) = copied {
                        let _ = fs::remove_file(&candidate);
                        return Ok(Outcome::Failed(describe(&anyhow::Error::from(e))));
                    }
                }
                return Ok(Outcome::Stored(digest));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if same_content(&candidate, entry.size, &digest, cancel) {
                    return Ok(Outcome::Identical);
                }
            }
            Err(e) => return Ok(Outcome::Failed(describe(&anyhow::Error::from(e)))),
        }
    }
    Ok(Outcome::Failed("No unique file name remains".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpListener, sync::Arc};

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "classroom-tree-{name}-{}-{}",
            std::process::id(),
            media_core::unix_us()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn stream(plan: &Plan, sink: &Sink) -> (Progress, Progress) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let receiving = Arc::clone(&cancel);
        let flat = sink.flat;
        let root = sink.root.clone();
        let receiver = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let sink = Sink {
                root,
                flat,
                tag: "test".into(),
            };
            receive_tree(&mut s, &sink, &receiving, &mut |_| {}).unwrap()
        });
        let mut s = TcpStream::connect(address).unwrap();
        let sent = send_tree(&mut s, plan, Duration::ZERO, &cancel, &mut |_| {}).unwrap();
        (sent, receiver.join().unwrap())
    }

    #[test]
    fn extension_list_is_normalised() {
        assert_eq!(parse_exts("DOCX, .pdf; *.Py  pdf"), ["docx", "pdf", "py"]);
        assert!(parse_exts("  ").is_empty());
    }

    #[test]
    fn plan_filters_extensions_depth_hidden_and_size() {
        let src = temp("plan");
        write(&src.join("a.docx"), "a");
        write(&src.join("b.txt"), "b");
        write(&src.join("sub/c.DOCX"), "c");
        write(&src.join(".hidden.docx"), "h");
        write(&src.join("~$lock.docx"), "l");
        write(&src.join("big.docx"), &"x".repeat(2 * 1024 * 1024));
        let cancel = AtomicBool::new(false);
        let filter = Filter {
            exts: parse_exts("docx"),
            max_file_mb: 1,
            ..Filter::default()
        };
        let p = plan(&[src.clone()], &filter, false, &cancel).unwrap();
        let mut rels: Vec<_> = p.items.iter().map(|i| i.rel.as_str()).collect();
        rels.sort_unstable();
        assert_eq!(rels, ["a.docx", "sub/c.DOCX"]);
        assert_eq!(
            p.skipped, 1,
            "oversized file is reported, not silently lost"
        );
        let shallow = Filter {
            recursive: false,
            ..filter
        };
        let p = plan(&[src.clone()], &shallow, false, &cancel).unwrap();
        assert_eq!(p.files, 1);
        fs::remove_dir_all(src).unwrap();
    }

    #[test]
    fn stream_keeps_structure_and_never_overwrites() {
        let src = temp("src");
        let dst = temp("dst");
        write(&src.join("root.txt"), "root");
        write(&src.join("dir/nested/deep.txt"), "deep");
        write(&src.join("dir/root.txt"), "other");
        fs::create_dir_all(src.join("empty")).unwrap();
        let cancel = AtomicBool::new(false);
        let p = plan(&[src.clone()], &Filter::default(), false, &cancel).unwrap();
        let sink = Sink {
            root: dst.join("PC-01"),
            flat: false,
            tag: "t".into(),
        };
        let (sent, got) = stream(&p, &sink);
        assert_eq!((sent.files_done, got.files_done, got.failed), (3, 3, 0));
        assert_eq!(
            fs::read_to_string(dst.join("PC-01/dir/nested/deep.txt")).unwrap(),
            "deep"
        );
        assert!(dst.join("PC-01/empty").is_dir());
        // Same content again is skipped, changed content is kept side by side.
        write(&src.join("root.txt"), "changed");
        let p = plan(&[src.clone()], &Filter::default(), false, &cancel).unwrap();
        let (_, again) = stream(&p, &sink);
        assert_eq!(again.skipped, 2);
        assert_eq!(
            fs::read_to_string(dst.join("PC-01/root.txt")).unwrap(),
            "root"
        );
        assert_eq!(
            fs::read_to_string(dst.join("PC-01/root-1.txt")).unwrap(),
            "changed"
        );
        assert!(!fs::read_dir(dst.join("PC-01")).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".part")));
        fs::remove_dir_all(src).unwrap();
        fs::remove_dir_all(dst).unwrap();
    }

    #[test]
    fn flat_mode_collapses_levels_and_numbers_collisions() {
        let src = temp("flat-src");
        let dst = temp("flat-dst");
        write(&src.join("a/report.txt"), "one");
        write(&src.join("b/report.txt"), "two");
        write(&src.join("b/c/report.txt"), "one");
        let cancel = AtomicBool::new(false);
        let p = plan(&[src.clone()], &Filter::default(), false, &cancel).unwrap();
        let sink = Sink {
            root: dst.join("PC"),
            flat: true,
            tag: "t".into(),
        };
        let (_, got) = stream(&p, &sink);
        assert_eq!(got.failed, 0);
        let mut names: Vec<_> = fs::read_dir(dst.join("PC"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["report-1.txt", "report.txt"]);
        fs::remove_dir_all(src).unwrap();
        fs::remove_dir_all(dst).unwrap();
    }

    #[test]
    fn hostile_relative_paths_cannot_escape_the_destination() {
        let sink = Sink {
            root: PathBuf::from("/dest/PC"),
            flat: false,
            tag: "t".into(),
        };
        for rel in ["../../etc/passwd", "a/../../b", "/abs/x", "..\\..\\x"] {
            let (dir, name) = sink.destination(rel).unwrap();
            assert!(dir.starts_with("/dest/PC"), "{rel} -> {dir:?}");
            assert!(!name.contains(['/', '\\']) && name != "..");
            assert!(!dir
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)));
        }
        assert!(sink.destination("").is_err());
    }

    #[test]
    fn listing_reports_entries_and_errors() {
        let dir = temp("list");
        write(&dir.join("f.txt"), "12345");
        fs::create_dir_all(dir.join("d")).unwrap();
        write(&dir.join(".secret"), "");
        let shown = list(dir.to_str().unwrap(), false);
        assert_eq!(shown["entries"].as_array().unwrap().len(), 2);
        let all = list(dir.to_str().unwrap(), true);
        assert_eq!(all["entries"].as_array().unwrap().len(), 3);
        assert!(list("relative/path", false)["error"].is_string());
        assert!(list(dir.join("missing").to_str().unwrap(), false)["error"].is_string());
        fs::remove_dir_all(dir).unwrap();
    }
}
