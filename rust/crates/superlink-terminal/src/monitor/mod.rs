//! Server health snapshots.
//!
//! Remote probes read /proc through a fixed, read-only command; parsing is
//! tolerant so a host without optional sensors still yields the basic
//! snapshot. Remote GPUs come from nvidia-smi when it is installed; other GPU
//! vendors are not probed. Local probes use sysinfo instead of shelling out.

use std::collections::BTreeMap;
use std::time::Instant;

pub mod commands;
pub mod local;
pub mod parse;
pub mod rates;
pub mod remote;

pub use commands::{large_files_command, stop_process_command};
pub use local::{LocalMonitor, MonitorSnapshot};
pub use parse::{parse_details, parse_metrics};
pub use rates::{MonitorRates, rates};
pub use remote::LargeFile;

/// A bounded Linux server health snapshot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metrics {
    pub system: String,
    pub load: String,
    pub uptime: String,
    pub disk: String,
    pub memory_total: u64,
    pub memory_used: u64,
    pub details: MonitorDetails,
}

/// Cumulative counters for one CPU, as read from /proc/stat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CpuCounter {
    pub name: String,
    pub user: u64,
    pub system: u64,
    pub idle: u64,
    pub wait: u64,
    pub total: u64,
}

/// Cumulative interface counters from /proc/net/dev.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkCounter {
    pub name: String,
    pub received: u64,
    pub sent: u64,
}

/// Cumulative disk counters from /proc/diskstats.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiskCounter {
    pub name: String,
    pub reads: u64,
    pub writes: u64,
}

/// A mounted filesystem.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Partition {
    pub device: String,
    pub mount: String,
    pub total: u64,
    pub used: u64,
    pub percent: f64,
}

/// A top process, without its command line arguments.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcessMetric {
    pub pid: i32,
    pub user: String,
    pub name: String,
    pub cpu: f64,
    pub memory: f64,
    pub rss: u64,
    /// Kernel start ticks, used to reject PID reuse before signalling.
    pub start: u64,
}

/// A listening socket.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listener {
    pub protocol: String,
    pub address: String,
}

/// GPU usage reported by nvidia-smi, when installed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gpu {
    pub name: String,
    pub used: u64,
    pub total: u64,
    pub utilization: f64,
    pub temperature: f64,
}

/// Optional probe sections; an unavailable sensor never invalidates the
/// basic snapshot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MonitorDetails {
    pub sampled: Option<Instant>,
    pub connections: u64,
    pub tcp_connections: u64,
    pub time_wait: u64,
    pub udp_connections: u64,
    pub handles: u64,
    pub handle_limit: u64,
    pub listeners: Vec<Listener>,
    pub addresses: BTreeMap<String, String>,
    pub kernel: String,
    pub cpus: Vec<CpuCounter>,
    pub networks: Vec<NetworkCounter>,
    pub disks: Vec<DiskCounter>,
    pub partitions: Vec<Partition>,
    pub processes: Vec<ProcessMetric>,
    pub gpus: Vec<Gpu>,
    pub memory_free: u64,
    pub memory_available: u64,
    pub cached: u64,
    pub buffers: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub temperature: f64,
}

/// Hard cap for a single probe response.
pub const MAX_MONITOR_OUTPUT: usize = 256 << 10;
