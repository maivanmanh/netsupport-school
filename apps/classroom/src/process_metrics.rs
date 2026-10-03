use anyhow::{anyhow, Result};
use sysinfo::{
    get_current_pid, CpuRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, System,
};

pub struct ProcessMetrics {
    system: System,
    pid: Pid,
    available: bool,
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
        let mut metrics = Self {
            system: System::new(),
            pid,
            available: true,
        };
        if let Err(error) = metrics.refresh() {
            metrics.available = false;
            tracing::warn!(%error, "process telemetry unavailable; media continues, placeholder values marked unavailable");
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
