//! Screenshots taken as evidence that a Student used an AI website.
//!
//! **Student:** [`Watcher`] looks at what the Student is doing (from the activity
//! report) and, at the moment [`crate::activity::evidence::Planner`] chooses,
//! takes a screenshot, shrinks it to a JPEG and sends it to the Teacher over a
//! short connection of its own. **Teacher:** [`Store`] accepts those pictures,
//! keeps today's in a folder of the Teacher's profile and forgets them at
//! midnight, so the disk never fills up with old evidence.
use super::{configure, nonce, packet, read, send, Hub};
use crate::{
    activity::{
        address_of,
        evidence::{Counts, Plan, Planner, Seen, Settings},
        ActivityOptions, Report,
    },
    ai_sites::Matcher,
};
use anyhow::{Context, Result};
use protocol::ToolPacket;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddrV4, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

/// Largest picture accepted.
const MAX_BYTES: u64 = 4 * 1024 * 1024;
/// Most pictures kept for the whole class in a day, and their total size.
const MAX_ITEMS_PER_DAY: usize = 800;
const MAX_BYTES_PER_DAY: u64 = 1024 * 1024 * 1024;
/// The Teacher refuses new pictures when its disk has less than this free.
const MIN_FREE_BYTES: u64 = 512 * 1024 * 1024;

// ------------------------------------------------------------------ Student

/// Watches the Student's screen state and decides when to take a picture.
pub(super) struct Watcher {
    planner: Planner,
    matcher: Matcher,
    watch: Vec<String>,
    settings: Settings,
    enabled: bool,
    counts_path: PathBuf,
    busy: Arc<AtomicBool>,
}

impl Watcher {
    pub fn new(state_dir: &Path) -> Self {
        let counts_path = state_dir.join("ai-evidence.json");
        let counts: Counts = fs::read(&counts_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            planner: Planner::restore(&counts, crate::local_day::today()),
            matcher: Matcher::default(),
            watch: Vec::new(),
            settings: Settings {
                per_site: 3,
                first_seconds: 60,
                gap_seconds: 180,
            },
            enabled: false,
            counts_path,
            busy: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The Teacher's current choices (arrive with the monitoring switch).
    pub fn configure(&mut self, options: &ActivityOptions) {
        self.enabled = options.evidence && !options.watch.is_empty();
        self.settings = Settings {
            per_site: options.evidence_per_site,
            first_seconds: u64::from(options.evidence_first_seconds),
            gap_seconds: u64::from(options.evidence_gap_seconds),
        };
        if self.watch != options.watch {
            self.watch.clone_from(&options.watch);
            self.matcher = Matcher::new(&options.watch);
        }
    }

    /// Looks at the latest activity report; returns a plan if a picture is due.
    pub fn observe(&mut self, report: &Report, now_ms: i64) -> Option<Plan> {
        if !self.enabled || !report.monitoring_enabled || self.busy.load(Ordering::Acquire) {
            return None;
        }
        // The page of the front window, only if that window is a browser.
        let front = report.windows.first()?;
        let tab = report
            .browsers
            .iter()
            .find(|b| b.app.eq_ignore_ascii_case(&front.app))
            .and_then(|b| b.tabs.iter().find(|t| t.active && t.url.is_some()));
        let address = tab.and_then(|t| t.url.as_deref()).and_then(address_of);
        let seen = Seen {
            now_ms,
            address: address.as_deref(),
            title: tab.map_or("", |t| t.title.as_str()),
            idle_ms: idle_ms(),
        };
        let plan = self.planner.observe(
            &self.matcher,
            &seen,
            &self.settings,
            crate::local_day::today(),
        )?;
        if let Ok(bytes) = serde_json::to_vec(&self.planner.counts()) {
            let _ = fs::write(&self.counts_path, bytes);
        }
        Some(plan)
    }

    /// Takes the picture and sends it, off the connection's thread.
    pub fn capture(
        &self,
        plan: Plan,
        teacher: SocketAddrV4,
        identity: String,
        state_dir: PathBuf,
        cancel: Arc<AtomicBool>,
    ) {
        if self.busy.swap(true, Ordering::AcqRel) {
            return;
        }
        let busy = Arc::clone(&self.busy);
        thread::spawn(move || {
            let outcome = take_and_send(&plan, teacher, &identity, &state_dir, &cancel);
            match outcome {
                Ok(()) => {
                    tracing::info!(site = %plan.label, reason = plan.reason, "AI evidence sent")
                }
                Err(error) => tracing::warn!(%error, site = %plan.label, "AI evidence not sent"),
            }
            busy.store(false, Ordering::Release);
        });
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn idle_ms() -> Option<u64> {
    use windows::Win32::{
        System::SystemInformation::GetTickCount,
        UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
    };
    let mut info = LASTINPUTINFO {
        cbSize: u32::try_from(std::mem::size_of::<LASTINPUTINFO>()).ok()?,
        dwTime: 0,
    };
    // SAFETY: `info` is a valid, correctly sized out-structure.
    unsafe {
        GetLastInputInfo(&mut info).ok().ok()?;
        Some(u64::from(GetTickCount().wrapping_sub(info.dwTime)))
    }
}
#[cfg(not(windows))]
fn idle_ms() -> Option<u64> {
    None
}

fn take_and_send(
    plan: &Plan,
    teacher: SocketAddrV4,
    identity: &str,
    state_dir: &Path,
    cancel: &AtomicBool,
) -> Result<()> {
    let folder = state_dir.join("evidence-tmp");
    fs::create_dir_all(&folder)?;
    let stem = nonce()?;
    let png = folder.join(format!("{stem}.png"));
    let jpeg = folder.join(format!("{stem}.jpg"));
    let result = (|| {
        super::screenshot::capture(&png, cancel)?;
        png_to_jpeg(&png, &jpeg)?;
        upload(plan, teacher, identity, &jpeg)
    })();
    let _ = fs::remove_file(&png);
    let _ = fs::remove_file(&jpeg);
    result
}

/// Evidence is for reading, not for printing: a JPEG of about 0.2–0.4 MB.
pub(super) fn png_to_jpeg(png_path: &Path, jpeg_path: &Path) -> Result<()> {
    let mut decoder = png::Decoder::new(fs::File::open(png_path)?);
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer)?;
    let (width, height) = (info.width, info.height);
    anyhow::ensure!(
        width > 0 && height > 0 && width <= 4096 && height <= 4096,
        "Invalid screenshot size"
    );
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => anyhow::bail!("Unsupported screenshot format {other:?}"),
    };
    let mut rgb = Vec::with_capacity(usize::try_from(width * height)? * 3);
    for pixel in buffer[..info.buffer_size()].chunks_exact(channels) {
        rgb.extend_from_slice(&pixel[..3]);
    }
    let encoder = jpeg_encoder::Encoder::new_file(jpeg_path, 72)?;
    encoder.encode(
        &rgb,
        u16::try_from(width)?,
        u16::try_from(height)?,
        jpeg_encoder::ColorType::Rgb,
    )?;
    Ok(())
}

fn upload(plan: &Plan, teacher: SocketAddrV4, identity: &str, jpeg: &Path) -> Result<()> {
    let bytes = fs::read(jpeg)?;
    anyhow::ensure!(
        !bytes.is_empty() && bytes.len() as u64 <= MAX_BYTES,
        "Evidence picture has an unusable size"
    );
    let mut s = TcpStream::connect_timeout(&teacher.into(), Duration::from_secs(3))?;
    configure(&s)?;
    s.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut p = packet("evidence_upload");
    p.student = identity.to_owned();
    p.id = nonce()?;
    p.size = bytes.len() as u64;
    p.hash = format!("{:x}", Sha256::digest(&bytes));
    p.text = json!({
        "label": plan.label,
        "address": plan.address,
        "title": plan.title.chars().take(120).collect::<String>(),
        "reason": plan.reason,
        "taken_ms": crate::activity::now_ms(),
    })
    .to_string();
    send(&mut s, &p)?;
    let answer = read(&mut s)?;
    anyhow::ensure!(
        answer.action == "ready",
        "Teacher did not accept the evidence: {}",
        answer.text
    );
    s.write_all(&bytes)?;
    anyhow::ensure!(
        read(&mut s)?.action == "complete",
        "Teacher did not confirm the evidence"
    );
    Ok(())
}

// ------------------------------------------------------------------ Teacher

/// One picture kept by the Teacher.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub student: String,
    /// The AI site (or part of a site) it shows.
    pub label: String,
    pub address: String,
    pub title: String,
    pub reason: String,
    pub taken_ms: i64,
    /// Path below the evidence folder.
    pub file: String,
    pub bytes: u64,
}

/// Today's evidence, on disk and in memory.
#[derive(Default)]
pub struct Store {
    root: PathBuf,
    day: u32,
    items: Vec<Item>,
}

impl Store {
    fn day_dir(&self) -> PathBuf {
        self.root.join(self.day.to_string())
    }
    fn index_path(&self) -> PathBuf {
        self.day_dir().join("index.json")
    }
    fn save(&self) {
        let _ = fs::create_dir_all(self.day_dir());
        if let Ok(bytes) = serde_json::to_vec(&self.items) {
            let _ = fs::write(self.index_path(), bytes);
        }
    }

    /// Opens the evidence folder: removes every day but `today` and loads today's index.
    pub fn open(root: &Path, today: u32) -> Self {
        let _ = fs::create_dir_all(root);
        let mut store = Self {
            root: root.to_owned(),
            day: today,
            items: Vec::new(),
        };
        store.purge_other_days();
        store.items = fs::read(store.index_path())
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<Item>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|i| store.day_dir().join(&i.file).is_file())
            .collect();
        store
    }

    /// Everything from earlier days is deleted - evidence is kept for the day only.
    fn purge_other_days(&self) {
        let keep = self.day.to_string();
        if let Ok(entries) = fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy() != keep.as_str() {
                    let _ = fs::remove_dir_all(entry.path());
                }
            }
        }
    }

    /// Called often: when midnight passes, yesterday's pictures go.
    pub fn roll_day(&mut self, today: u32) -> bool {
        if self.root.as_os_str().is_empty() || today == self.day {
            return false;
        }
        self.day = today;
        self.items.clear();
        self.purge_other_days();
        true
    }

    pub fn enabled(&self) -> bool {
        !self.root.as_os_str().is_empty()
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn count_for(&self, student: &str, label: &str) -> usize {
        self.items
            .iter()
            .filter(|i| i.student == student && i.label == label)
            .count()
    }

    /// May this picture be accepted? `Err` carries the reason shown to nobody
    /// but the log: the Student simply drops it.
    pub fn admit(
        &self,
        student: &str,
        label: &str,
        size: u64,
        per_site: u32,
    ) -> Result<(), &'static str> {
        if !self.enabled() {
            return Err("evidence is switched off");
        }
        if size == 0 || size > MAX_BYTES {
            return Err("picture size not allowed");
        }
        if self.count_for(student, label) >= per_site as usize {
            return Err("allowance for this site is used up");
        }
        if self.items.len() >= MAX_ITEMS_PER_DAY
            || self.items.iter().map(|i| i.bytes).sum::<u64>() + size > MAX_BYTES_PER_DAY
        {
            return Err("daily limit reached");
        }
        if fs2::available_space(&self.root).is_ok_and(|free| free < MIN_FREE_BYTES + size) {
            return Err("Teacher disk is nearly full");
        }
        Ok(())
    }

    /// Where a new picture is written.
    fn path_for(&self, student: &str, label: &str, id: &str) -> (String, PathBuf) {
        let name = format!(
            "{}-{}-{}.jpg",
            super::safe_name(label).replace('/', "_"),
            self.count_for(student, label) + 1,
            &id[..id.len().min(8)]
        );
        let relative = format!("{}/{name}", super::safe_name(student));
        let full = self.day_dir().join(&relative);
        (relative, full)
    }

    fn add(&mut self, item: Item) {
        self.items.push(item);
        self.save();
    }

    pub fn delete(&mut self, ids: &[String]) -> usize {
        let before = self.items.len();
        let dir = self.day_dir();
        self.items.retain(|item| {
            let remove = ids.contains(&item.id);
            if remove {
                let _ = fs::remove_file(dir.join(&item.file));
            }
            !remove
        });
        if self.items.len() != before {
            self.save();
        }
        before - self.items.len()
    }

    /// For the console: newest first, with absolute paths.
    pub fn snapshot(&self) -> Value {
        let dir = self.day_dir();
        let mut items: Vec<&Item> = self.items.iter().collect();
        items.sort_by_key(|i| std::cmp::Reverse(i.taken_ms));
        let total = items.len();
        // The console is sent a snapshot every second: only the newest are listed.
        items.truncate(300);
        json!({
            "day": self.day,
            "folder": dir,
            "total": total,
            "items": items.iter().map(|i| json!({
                "id": i.id, "student": i.student, "label": i.label, "address": i.address,
                "title": i.title, "reason": i.reason, "taken_ms": i.taken_ms,
                "bytes": i.bytes, "path": dir.join(&i.file),
            })).collect::<Vec<_>>(),
        })
    }
}

impl Hub {
    pub fn set_evidence_dir(&self, root: &Path) {
        self.0.lock().unwrap().evidence = Store::open(root, crate::local_day::today());
    }
    pub fn evidence_delete(&self, ids: &[String]) -> usize {
        self.0.lock().unwrap().evidence.delete(ids)
    }
    pub(super) fn evidence_snapshot(&self) -> Value {
        let mut state = self.0.lock().unwrap();
        state.evidence.roll_day(crate::local_day::today());
        state.evidence.snapshot()
    }
}

pub(super) fn serve(s: &mut TcpStream, p: &ToolPacket, hub: &Hub) -> Option<Result<()>> {
    (p.action == "evidence_upload").then(|| receive(s, p, hub))
}

fn receive(s: &mut TcpStream, p: &ToolPacket, hub: &Hub) -> Result<()> {
    let meta: Value = serde_json::from_str(&p.text).unwrap_or(Value::Null);
    let label = meta["label"].as_str().unwrap_or("").to_owned();
    let address = meta["address"].as_str().unwrap_or("").to_owned();
    let reply_no = |s: &mut TcpStream, why: &str| -> Result<()> {
        let mut no = packet("deny");
        no.text = why.to_owned();
        send(s, &no)
    };
    anyhow::ensure!(
        p.hash.len() == 64 && p.hash.bytes().all(|b| b.is_ascii_hexdigit()) && p.id.len() >= 8,
        "Invalid evidence metadata"
    );
    // Only sites the Teacher lists, so a stray request cannot invent evidence.
    let (allowed, per_site) = {
        let mut state = hub.0.lock().unwrap();
        state.evidence.roll_day(crate::local_day::today());
        let listed = state.ai_matcher.matching(&address).as_deref() == Some(label.as_str());
        let admit = state.evidence.admit(
            &p.student,
            &label,
            p.size,
            state.tuning.activity.evidence_per_site,
        );
        (
            listed.then_some(admit),
            state.tuning.activity.evidence_per_site,
        )
    };
    let _ = per_site;
    match allowed {
        None => return reply_no(s, "not a listed AI site"),
        Some(Err(why)) => return reply_no(s, why),
        Some(Ok(())) => {}
    }
    let (relative, full) = {
        let state = hub.0.lock().unwrap();
        state.evidence.path_for(&p.student, &label, &p.id)
    };
    fs::create_dir_all(full.parent().context("evidence folder")?)?;
    let temporary = full.with_extension("jpg.partial");
    send(s, &packet("ready"))?;
    let outcome = (|| -> Result<()> {
        let mut file = fs::File::create(&temporary)?;
        let mut hash = Sha256::new();
        let mut left = p.size;
        let mut buffer = vec![0; 64 * 1024];
        let mut first = [0_u8; 2];
        let mut seen = 0_usize;
        while left > 0 {
            let n = usize::try_from(left.min(buffer.len() as u64))?;
            s.read_exact(&mut buffer[..n])?;
            if seen < 2 {
                for (slot, byte) in first.iter_mut().skip(seen).zip(buffer[..n].iter()) {
                    *slot = *byte;
                }
                seen = (seen + n).min(2);
            }
            file.write_all(&buffer[..n])?;
            hash.update(&buffer[..n]);
            left -= n as u64;
        }
        file.sync_all()?;
        anyhow::ensure!(
            format!("{:x}", hash.finalize()) == p.hash,
            "Evidence checksum mismatch"
        );
        anyhow::ensure!(first == [0xFF, 0xD8], "Evidence is not a JPEG picture");
        fs::rename(&temporary, &full)?;
        Ok(())
    })();
    if let Err(error) = outcome {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    let item = Item {
        id: p.id.clone(),
        student: p.student.clone(),
        label,
        address,
        title: meta["title"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(120)
            .collect(),
        reason: meta["reason"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(24)
            .collect(),
        taken_ms: crate::activity::now_ms(),
        file: relative,
        bytes: p.size,
    };
    hub.0.lock().unwrap().evidence.add(item);
    send(s, &packet("complete"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("evidence-test-{}", nonce().unwrap()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn item(id: &str, student: &str, label: &str, file: &str) -> Item {
        Item {
            id: id.into(),
            student: student.into(),
            label: label.into(),
            address: label.into(),
            title: "t".into(),
            reason: "in_use".into(),
            taken_ms: 1,
            file: file.into(),
            bytes: 10,
        }
    }

    #[test]
    fn only_todays_evidence_survives_a_restart_and_older_days_are_deleted() {
        let root = temp();
        // Yesterday left a folder with a picture; today has one too.
        let old = root.join("20261015/PC-01");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("a.jpg"), b"x").unwrap();
        let today = root.join("20261016/PC-01");
        fs::create_dir_all(&today).unwrap();
        fs::write(today.join("b.jpg"), b"y").unwrap();
        fs::write(
            root.join("20261016/index.json"),
            serde_json::to_vec(&vec![
                item("1", "pc01", "chatgpt.com", "PC-01/b.jpg"),
                item("2", "pc01", "chatgpt.com", "PC-01/missing.jpg"),
            ])
            .unwrap(),
        )
        .unwrap();
        let store = Store::open(&root, 20261016);
        assert!(!root.join("20261015").exists(), "older days are removed");
        assert_eq!(
            store.items().len(),
            1,
            "an entry without its file is dropped"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn midnight_clears_everything_and_a_new_day_starts_empty() {
        let root = temp();
        let mut store = Store::open(&root, 20261016);
        let (rel, full) = store.path_for("pc01", "chatgpt.com", "abcdef123456");
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, b"jpeg").unwrap();
        store.add(item("1", "pc01", "chatgpt.com", &rel));
        assert!(root.join("20261016").exists());
        assert!(!store.roll_day(20261016), "same day: nothing happens");
        assert!(store.roll_day(20261017));
        assert!(store.items().is_empty());
        assert!(
            !root.join("20261016").exists(),
            "yesterday's pictures are gone"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_allowance_per_site_and_other_limits_are_enforced() {
        let root = temp();
        let mut store = Store::open(&root, 20261016);
        assert_eq!(store.admit("s", "chatgpt.com", 100, 3), Ok(()));
        for n in 0..3 {
            store.add(item(&n.to_string(), "s", "chatgpt.com", "x.jpg"));
        }
        assert!(store.admit("s", "chatgpt.com", 100, 3).is_err());
        assert_eq!(
            store.admit("s", "claude.ai", 100, 3),
            Ok(()),
            "another site"
        );
        assert_eq!(
            store.admit("other", "chatgpt.com", 100, 3),
            Ok(()),
            "another Student"
        );
        assert!(store.admit("s", "claude.ai", 0, 3).is_err());
        assert!(store.admit("s", "claude.ai", MAX_BYTES + 1, 3).is_err());
        assert!(
            Store::default().admit("s", "a", 10, 3).is_err(),
            "off until a folder is set"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deleting_removes_the_file_and_the_entry() {
        let root = temp();
        let mut store = Store::open(&root, 20261016);
        let (rel, full) = store.path_for("pc01", "chatgpt.com", "abcdef123456");
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, b"jpeg").unwrap();
        store.add(item("1", "pc01", "chatgpt.com", &rel));
        assert_eq!(store.delete(&["nope".into()]), 0);
        assert_eq!(store.delete(&["1".into()]), 1);
        assert!(!full.exists());
        assert!(store.items().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    fn tiny_jpeg(dir: &Path) -> PathBuf {
        let png_path = dir.join("shot.png");
        let jpeg_path = dir.join("shot.jpg");
        let mut encoder = png::Encoder::new(fs::File::create(&png_path).unwrap(), 64, 48);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let pixels: Vec<u8> = (0..64 * 48)
            .flat_map(|i| [(i % 251) as u8, 90, 200, 255])
            .collect();
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
        png_to_jpeg(&png_path, &jpeg_path).unwrap();
        jpeg_path
    }

    /// Runs the Teacher's side of one connection on a thread.
    fn teacher(hub: Arc<Hub>) -> (SocketAddrV4, thread::JoinHandle<Result<()>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = match listener.local_addr().unwrap() {
            std::net::SocketAddr::V4(a) => a,
            _ => unreachable!(),
        };
        let handle = thread::spawn(move || {
            let (mut s, _) = listener.accept()?;
            configure(&s)?;
            let hello = read(&mut s)?;
            serve(&mut s, &hello, &hub).context("not evidence")?
        });
        (addr, handle)
    }

    fn plan(label: &str) -> Plan {
        Plan {
            label: label.into(),
            address: format!("{label}/c/1"),
            title: "Bài tập".into(),
            reason: "conversation",
        }
    }

    #[test]
    fn a_picture_travels_from_student_to_teacher_and_is_listed() {
        let root = temp();
        let hub = Arc::new(Hub::default());
        hub.set_ai_sites(&["https://chatgpt.com".to_owned()]);
        hub.set_evidence_dir(&root);
        let jpeg = tiny_jpeg(&root);
        let (addr, server) = teacher(Arc::clone(&hub));
        upload(&plan("chatgpt.com"), addr, "pc01", &jpeg).unwrap();
        server.join().unwrap().unwrap();
        let snapshot = hub.evidence_snapshot();
        let items = snapshot["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["student"], "pc01");
        assert_eq!(items[0]["label"], "chatgpt.com");
        assert_eq!(items[0]["title"], "Bài tập");
        let path = PathBuf::from(items[0]["path"].as_str().unwrap());
        let bytes = fs::read(path).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
        assert_eq!(bytes.len() as u64, items[0]["bytes"].as_u64().unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_site_that_is_not_listed_or_is_over_its_allowance_is_refused() {
        let root = temp();
        let hub = Arc::new(Hub::default());
        hub.set_ai_sites(&["https://chatgpt.com".to_owned()]);
        hub.set_evidence_dir(&root);
        let jpeg = tiny_jpeg(&root);
        // Not on the Teacher's list.
        let (addr, server) = teacher(Arc::clone(&hub));
        let refused = upload(&plan("tinteo.vn"), addr, "pc01", &jpeg);
        assert!(refused
            .unwrap_err()
            .to_string()
            .contains("not a listed AI site"));
        server.join().unwrap().unwrap();
        // Over the allowance: three are kept, the fourth is turned away.
        for n in 0..4 {
            let (addr, server) = teacher(Arc::clone(&hub));
            let result = upload(&plan("chatgpt.com"), addr, "pc01", &jpeg);
            server.join().unwrap().unwrap();
            assert_eq!(result.is_ok(), n < 3, "picture {n}");
        }
        assert_eq!(
            hub.evidence_snapshot()["items"].as_array().unwrap().len(),
            3
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_corrupted_upload_is_discarded() {
        let root = temp();
        let hub = Arc::new(Hub::default());
        hub.set_ai_sites(&["https://chatgpt.com".to_owned()]);
        hub.set_evidence_dir(&root);
        let (addr, server) = teacher(Arc::clone(&hub));
        let mut s = TcpStream::connect(addr).unwrap();
        configure(&s).unwrap();
        let body = b"not really a picture";
        let mut p = packet("evidence_upload");
        p.student = "pc01".into();
        p.id = "abcdef123456".into();
        p.size = body.len() as u64;
        p.hash = format!("{:x}", Sha256::digest(body));
        p.text = json!({"label":"chatgpt.com","address":"chatgpt.com/c/1"}).to_string();
        send(&mut s, &p).unwrap();
        assert_eq!(read(&mut s).unwrap().action, "ready");
        s.write_all(body).unwrap();
        assert!(
            read(&mut s).is_err(),
            "no confirmation for something that is not a JPEG"
        );
        assert!(server.join().unwrap().is_err());
        assert!(hub.evidence_snapshot()["items"]
            .as_array()
            .unwrap()
            .is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
