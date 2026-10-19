//! Background collection, decoupled from the Teacher ever asking: while the
//! Teacher has activity monitoring switched on, a Student keeps its
//! [`history::History`] and a cached [`Report`] current on its own, so that
//! answering an on-demand request (`classroom_tools/activity_jobs.rs`) is
//! just handing over the already-current cache — no scan happens inside that
//! request/response round trip, which is what made opening the Activity
//! window feel slow before this existed.
use super::{history::History, Report, SharedOptions};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

/// Downloads (disk + SQLite) are read every Nth cycle — real I/O, so it runs
/// less often than the in-memory window/tab scan.
const DOWNLOADS_EVERY: u32 = 5;

/// Shared, cheap-to-read cache of the most recent report; `None` until the
/// collector's first cycle completes.
pub type Live = Arc<Mutex<Option<Report>>>;

pub struct Collector {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Collector {
    pub fn start(history: Arc<History>, live: Live, options: SharedOptions) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            let mut tick: u32 = 0;
            while !worker_stop.load(Ordering::Acquire) {
                let now = options.lock().unwrap().clone();
                let mut report = super::build(
                    &history,
                    now.collect_downloads && tick % DOWNLOADS_EVERY == 0,
                );
                if !now.collect_downloads {
                    report.downloads.clear();
                }
                *live.lock().unwrap() = Some(report);
                tick = tick.wrapping_add(1);
                let start = std::time::Instant::now();
                // Re-read each pass so a changed setting takes effect at once.
                while !worker_stop.load(Ordering::Acquire)
                    && start.elapsed()
                        < Duration::from_secs(u64::from(options.lock().unwrap().scan_seconds))
                {
                    thread::sleep(Duration::from_millis(200));
                }
            }
        });
        Self {
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Collector {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
