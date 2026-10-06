use anyhow::{anyhow, Result};
use sysinfo::{
    get_current_pid, CpuRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, System,
};

pub struct ProcessMetrics {
    system: System,
    pid: Pid,
    available: bool,
    #[cfg(target_os = "macos")]
    mac_previous: Option<MacTaskSample>,
    #[cfg(target_os = "macos")]
    mac_sampled_at: u64,
}

pub struct ProcessSample {
    /// A fully busy logical CPU is 100%; a multi-threaded process may exceed 100%.
    pub available: bool,
    pub cpu_percent: f32,
    pub working_set_bytes: u64,
}

impl ProcessMetrics {
    pub fn new() -> Result<Self> {
        let pid = get_current_pid().map_err(|error| anyhow!(error))?;
        #[cfg(target_os = "macos")]
        let mac_previous = mac_task_sample().ok();
        let mut metrics = Self {
            system: System::new(),
            pid,
            available: false,
            #[cfg(target_os = "macos")]
            mac_previous,
            #[cfg(target_os = "macos")]
            mac_sampled_at: mach_absolute_time(),
        };
        #[cfg(target_os = "macos")]
        if metrics.mac_previous.is_some() {
            metrics.available = true;
        } else if let Err(error) = metrics.refresh() {
            tracing::warn!(%error, "process telemetry unavailable; media continues, placeholder values marked unavailable");
        } else {
            metrics.available = true;
        }
        #[cfg(not(target_os = "macos"))]
        if let Err(error) = metrics.refresh() {
            tracing::warn!(%error, "process telemetry unavailable; media continues, placeholder values marked unavailable");
        } else {
            metrics.available = true;
        }
        metrics.system.refresh_cpu_list(CpuRefreshKind::nothing());
        metrics.system.refresh_memory();
        tracing::info!(
            os_name = ?System::name(),
            os_version = ?System::long_os_version(),
            architecture = std::env::consts::ARCH,
            process_id = std::process::id(),
            cpu_model = metrics.system.cpus().first().map_or("unknown", sysinfo::Cpu::brand),
            logical_cpus = metrics.system.cpus().len(),
            physical_cores = ?metrics.system.physical_core_count(),
            total_memory_bytes = metrics.system.total_memory(),
            "media host capabilities"
        );
        Ok(metrics)
    }

    pub fn sample(&mut self) -> Result<ProcessSample> {
        #[cfg(target_os = "macos")]
        if let Ok(current) = mac_task_sample() {
            let now = mach_absolute_time();
            let cpu_percent = self.mac_previous.map_or(0.0, |previous| {
                let elapsed_ticks = now.saturating_sub(self.mac_sampled_at);
                if elapsed_ticks == 0 {
                    return 0.0;
                }
                let process_ticks = current
                    .cpu_time_ticks
                    .saturating_sub(previous.cpu_time_ticks);
                (process_ticks as f64 / elapsed_ticks as f64 * 100.0) as f32
            });
            self.mac_previous = Some(current);
            self.mac_sampled_at = now;
            self.available = true;
            return Ok(ProcessSample {
                available: true,
                cpu_percent,
                working_set_bytes: current.resident_bytes,
            });
        }

        if !self.available || self.refresh().is_err() {
            self.available = false;
            return Ok(ProcessSample {
                available: false,
                cpu_percent: 0.0,
                working_set_bytes: 0,
            });
        }
        let process = self
            .system
            .process(self.pid)
            .ok_or_else(|| anyhow!("current process disappeared from process telemetry"))?;
        Ok(ProcessSample {
            available: true,
            cpu_percent: process.cpu_usage(),
            working_set_bytes: process.memory(),
        })
    }

    fn refresh(&mut self) -> Result<()> {
        let updated = self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        if updated == 0 {
            return Err(anyhow!("could not sample current process telemetry"));
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct MacTaskSample {
    cpu_time_ticks: u64,
    resident_bytes: u64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct TaskAbsoluteTimeInfo {
    total_user: u64,
    total_system: u64,
    _threads_user: u64,
    _threads_system: u64,
}

#[cfg(target_os = "macos")]
#[allow(deprecated, unsafe_code)] // Small Apple Mach boundary; callers receive copied metrics.
fn mac_task_sample() -> Result<MacTaskSample> {
    let mut absolute_info = std::mem::MaybeUninit::<TaskAbsoluteTimeInfo>::zeroed();
    let mut absolute_count = (std::mem::size_of::<TaskAbsoluteTimeInfo>()
        / std::mem::size_of::<libc::natural_t>())
        as libc::mach_msg_type_number_t;
    // TASK_ABSOLUTETIME_INFO includes CPU time for live and terminated threads.
    // SAFETY: the local structure matches Apple's four-u64 task_absolutetime_info ABI.
    let absolute_result = unsafe {
        libc::task_info(
            libc::mach_task_self(),
            1, // TASK_ABSOLUTETIME_INFO
            absolute_info.as_mut_ptr().cast::<libc::integer_t>(),
            &mut absolute_count,
        )
    };
    anyhow::ensure!(
        absolute_result == libc::KERN_SUCCESS,
        "mach task absolute-time query failed: {absolute_result}"
    );
    // SAFETY: a successful task_info call initialized the complete structure.
    let absolute_info = unsafe { absolute_info.assume_init() };

    let mut info = std::mem::MaybeUninit::<libc::mach_task_basic_info_data_t>::zeroed();
    let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
    // SAFETY: `info` is writable storage for MACH_TASK_BASIC_INFO_COUNT words;
    // task_info fills the structure for this process task returned by mach_task_self.
    #[allow(deprecated)]
    let result = unsafe {
        libc::task_info(
            libc::mach_task_self(),
            libc::MACH_TASK_BASIC_INFO,
            info.as_mut_ptr().cast::<libc::integer_t>(),
            &mut count,
        )
    };
    anyhow::ensure!(
        result == libc::KERN_SUCCESS,
        "mach task_info failed: {result}"
    );
    // SAFETY: a successful task_info call initialized the complete structure.
    let info = unsafe { info.assume_init() };
    // The SDK declares mach_task_basic_info with 4-byte packing; copy the packed
    // resident-size field with an unaligned read before accessing its value.
    let resident_bytes = unsafe { std::ptr::addr_of!(info.resident_size).read_unaligned() };
    Ok(MacTaskSample {
        cpu_time_ticks: absolute_info.total_user + absolute_info.total_system,
        resident_bytes,
    })
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn mach_absolute_time() -> u64 {
    // TASK_ABSOLUTETIME_INFO totals and this wall clock use Mach ticks, so their
    // ratio is independent of the host's Mach timebase conversion.
    #[allow(deprecated)]
    unsafe {
        libc::mach_absolute_time()
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::ProcessMetrics;
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    #[test]
    fn process_cpu_sample_includes_live_thread_work() {
        let mut metrics = ProcessMetrics::new().expect("initialize process metrics");
        let end = Instant::now() + Duration::from_millis(250);
        let mut value = 1_u64;
        while Instant::now() < end {
            value = value.wrapping_mul(6364136223846793005).wrapping_add(1);
            black_box(value);
        }
        let sample = metrics.sample().expect("sample process metrics");
        assert!(sample.available);
        assert!(sample.working_set_bytes > 0);
        assert!(
            sample.cpu_percent > 10.0,
            "CPU sample was {}%",
            sample.cpu_percent
        );
    }
}
