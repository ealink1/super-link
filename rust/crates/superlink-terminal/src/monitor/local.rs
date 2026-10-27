//! Local host snapshot through sysinfo.
//!
//! CPU, memory, partitions and network counters come from sysinfo, so the
//! local probe never shells out. Cores report total utilization in
//! [MonitorRates::cpus]; sysinfo exposes neither per-mode counters nor GPU
//! data, so [MonitorDetails::cpus] and [MonitorDetails::gpus] stay empty for
//! local hosts.

use std::time::Instant;

use sysinfo::{
    CpuRefreshKind, Disks, MemoryRefreshKind, Networks, ProcessesToUpdate, RefreshKind, System,
};

use crate::monitor::rates::rates as compute_rates;
use crate::monitor::rates::{CpuRate, MonitorRates};
use crate::monitor::{Metrics, MonitorDetails, NetworkCounter, Partition, ProcessMetric};

/// A local snapshot plus the rates derived from the previous one.
#[derive(Debug, Clone)]
pub struct MonitorSnapshot {
    pub metrics: Metrics,
    pub rates: MonitorRates,
}

/// Keeps sysinfo state so successive snapshots can report rates.
pub struct LocalMonitor {
    system: System,
    disks: Disks,
    networks: Networks,
    previous: Option<MonitorDetails>,
}

impl Default for LocalMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalMonitor {
    pub fn new() -> Self {
        let mut system = System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        // The first CPU sample only establishes a baseline.
        system.refresh_cpu_all();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        system.refresh_cpu_all();
        Self {
            system,
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            previous: None,
        }
    }

    /// Refreshes every source and returns the current snapshot.
    pub fn snapshot(&mut self) -> MonitorSnapshot {
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
        self.system.refresh_processes(ProcessesToUpdate::All);
        self.disks.refresh();
        self.networks.refresh();

        let sampled = Instant::now();
        let mut details = MonitorDetails {
            sampled: Some(sampled),
            kernel: System::kernel_version().unwrap_or_default(),
            memory_free: self.system.free_memory(),
            memory_available: self.system.available_memory(),
            swap_total: self.system.total_swap(),
            swap_used: self.system.used_swap(),
            ..MonitorDetails::default()
        };
        details.partitions = self.partitions();
        details.networks = self.network_counters();
        details.processes = self.processes();

        let mut current_rates = match &self.previous {
            Some(previous) => compute_rates(previous, &details),
            None => MonitorRates::default(),
        };
        current_rates.cpus = self.cpu_rates();
        current_rates.ready = self.previous.is_some();

        let load = System::load_average();
        let metrics = Metrics {
            system: System::name().unwrap_or_else(|| "local".to_owned()),
            load: format!("{:.2} {:.2} {:.2}", load.one, load.five, load.fifteen),
            uptime: format_uptime(System::uptime()),
            disk: self.disk_summary(),
            memory_total: self.system.total_memory(),
            memory_used: self.system.used_memory(),
            details: details.clone(),
        };
        self.previous = Some(details);
        MonitorSnapshot {
            metrics,
            rates: current_rates,
        }
    }

    fn cpu_rates(&self) -> Vec<CpuRate> {
        self.system
            .cpus()
            .iter()
            .map(|cpu| CpuRate {
                name: cpu.name().to_owned(),
                used: f64::from(cpu.cpu_usage()).clamp(0.0, 100.0),
                user: 0.0,
                system: 0.0,
                wait: 0.0,
            })
            .collect()
    }

    fn partitions(&self) -> Vec<Partition> {
        self.disks
            .list()
            .iter()
            .map(|disk| {
                let total = disk.total_space();
                let used = total.saturating_sub(disk.available_space());
                let percent = if total == 0 {
                    0.0
                } else {
                    100.0 * used as f64 / total as f64
                };
                Partition {
                    device: disk.name().to_string_lossy().into_owned(),
                    mount: disk.mount_point().to_string_lossy().into_owned(),
                    total,
                    used,
                    percent,
                }
            })
            .collect()
    }

    fn network_counters(&self) -> Vec<NetworkCounter> {
        self.networks
            .iter()
            .map(|(name, data)| NetworkCounter {
                name: name.clone(),
                received: data.total_received(),
                sent: data.total_transmitted(),
            })
            .collect()
    }

    fn processes(&self) -> Vec<ProcessMetric> {
        let mut processes: Vec<ProcessMetric> = self
            .system
            .processes()
            .values()
            .map(|process| ProcessMetric {
                pid: process.pid().as_u32() as i32,
                name: process.name().to_string_lossy().into_owned(),
                cpu: f64::from(process.cpu_usage()),
                memory: 0.0,
                rss: process.memory(),
                start: 0,
                ..ProcessMetric::default()
            })
            .collect();
        processes.sort_by(|left, right| {
            right
                .cpu
                .partial_cmp(&left.cpu)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        processes.truncate(20);
        processes
    }

    fn disk_summary(&self) -> String {
        let Some(disk) = self
            .disks
            .list()
            .iter()
            .find(|disk| disk.mount_point() == std::path::Path::new("/"))
            .or_else(|| self.disks.list().first())
        else {
            return String::new();
        };
        let total = disk.total_space();
        let available = disk.available_space();
        let used = total.saturating_sub(available);
        let percent = if total == 0 {
            0.0
        } else {
            100.0 * used as f64 / total as f64
        };
        format!("{percent:.0}% 已用 · {} 可用", human_bytes(available))
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}天 {hours}小时")
    } else if hours > 0 {
        format!("{hours}小时 {minutes}分钟")
    } else {
        format!("{minutes}分钟")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_snapshots_are_bounded_and_report_rates() {
        let mut monitor = LocalMonitor::new();
        let first = monitor.snapshot();
        assert!(first.metrics.memory_total > 0);
        assert!(!first.metrics.system.is_empty());
        assert!(!first.metrics.uptime.is_empty());
        assert!(first.metrics.details.sampled.is_some());
        assert!(first.metrics.details.processes.len() <= 20);
        assert!(!first.rates.ready);

        let second = monitor.snapshot();
        assert!(second.rates.ready);
        assert!(!second.rates.cpus.is_empty());
        assert!(
            second
                .rates
                .cpus
                .iter()
                .all(|cpu| (0.0..=100.0).contains(&cpu.used))
        );
    }

    #[test]
    fn uptime_and_sizes_are_human_readable() {
        assert_eq!(format_uptime(90), "1分钟");
        assert_eq!(format_uptime(3_700), "1小时 1分钟");
        assert_eq!(format_uptime(90_000), "1天 1小时");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KiB");
    }
}
