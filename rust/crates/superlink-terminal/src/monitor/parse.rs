//! Parsing of the fixed /proc probe response.
//!
//! Every optional section is independent: unknown or malformed lines are
//! skipped instead of failing the whole snapshot.

use std::time::Instant;

use crate::error::{Result, TerminalError};
use crate::monitor::{
    CpuCounter, DiskCounter, Gpu, Listener, MAX_MONITOR_OUTPUT, Metrics, MonitorDetails,
    NetworkCounter, Partition, ProcessMetric,
};

const SECTION_PREFIX: char = '@';
const MAX_CPUS: usize = 257;
const MAX_INTERFACES: usize = 128;
const MAX_DISKS: usize = 128;
const MAX_PARTITIONS: usize = 128;
const MAX_PROCESSES: usize = 40;
const MAX_GPUS: usize = 16;
const MAX_LISTENERS: usize = 100;
const MAX_ADDRESSES: usize = 128;

/// Parses the whole probe response, including optional details.
pub fn parse_metrics(raw: &str) -> Result<Metrics> {
    if raw.len() > MAX_MONITOR_OUTPUT {
        return Err(TerminalError::MonitorOutputTooLarge);
    }
    let (basic, details) = match raw.split_once("__DETAILS__") {
        Some((basic, details)) => (basic, Some(details)),
        None => (raw, None),
    };
    let sections: Vec<&str> = basic.split("__").collect();
    if sections.len() != 9 {
        return Err(TerminalError::MonitorFormat);
    }
    let mut metrics = Metrics {
        system: sections[0].trim().to_owned(),
        load: sections[2].trim().to_owned(),
        uptime: sections[6].trim().to_owned(),
        disk: sections[8].trim().to_owned(),
        ..Metrics::default()
    };
    if metrics.system != "Linux" {
        return Err(TerminalError::Unsupported(format!(
            "当前监控探针支持 Linux，此主机为 {}",
            metrics.system
        )));
    }
    let memory = sections[4];
    let mut total = 0;
    let mut available = None;
    for line in memory.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            continue;
        }
        let Some(value) = number(fields[1]).checked_mul(1024) else {
            continue;
        };
        match fields[0] {
            "MemTotal:" => total = value,
            "MemAvailable:" => available = Some(value),
            _ => {}
        }
    }
    let available = available.ok_or(TerminalError::MonitorFormat)?;
    if total == 0 || available > total {
        return Err(TerminalError::Validation("服务器内存指标无效".into()));
    }
    metrics.memory_total = total;
    metrics.memory_used = total - available;
    if let Some(details) = details {
        metrics.details = parse_details(details, memory);
    }
    Ok(metrics)
}

/// Parses the optional sections into bounded structures.
pub fn parse_details(raw: &str, memory: &str) -> MonitorDetails {
    let mut details = MonitorDetails {
        sampled: Some(Instant::now()),
        ..MonitorDetails::default()
    };
    parse_memory(memory, &mut details);
    let mut section = String::new();
    for line in raw.lines() {
        if let Some(name) = line.strip_prefix(SECTION_PREFIX) {
            section = name.trim().to_owned();
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        match section.as_str() {
            "KERNEL" => details.kernel = line.trim().to_owned(),
            "CPU" => parse_cpu(&fields, &mut details),
            "NET" => parse_network(line, &mut details),
            "IO" => parse_disk(&fields, &mut details),
            "PART" => parse_partition(&fields, &mut details),
            "PROC" => parse_process(&fields, &mut details),
            "GPU" => parse_gpu(line, &mut details),
            "SOCKET" => parse_sockets(&fields, &mut details),
            "HANDLES" => parse_handles(&fields, &mut details),
            "PORT" => parse_listener(&fields, &mut details),
            "ADDR" => parse_address(&fields, &mut details),
            "TEMP" => {
                let value = decimal(line) / 1000.0;
                if value > 0.0 && value < 150.0 && value > details.temperature {
                    details.temperature = value;
                }
            }
            _ => {}
        }
    }
    details
}

fn parse_memory(memory: &str, details: &mut MonitorDetails) {
    let mut swap_free = 0u64;
    for line in memory.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 {
            continue;
        }
        let Some(value) = number(fields[1]).checked_mul(1024) else {
            continue;
        };
        match fields[0] {
            "MemFree:" => details.memory_free = value,
            "MemAvailable:" => details.memory_available = value,
            "Cached:" => details.cached = value,
            "Buffers:" => details.buffers = value,
            "SwapTotal:" => details.swap_total = value,
            "SwapFree:" => swap_free = value,
            _ => {}
        }
    }
    details.swap_used = details.swap_total.saturating_sub(swap_free);
}

fn parse_cpu(fields: &[&str], details: &mut MonitorDetails) {
    if fields.len() < 9 || !fields[0].starts_with("cpu") || details.cpus.len() >= MAX_CPUS {
        return;
    }
    let mut cpu = CpuCounter {
        name: fields[0].to_owned(),
        user: number(fields[1]) + number(fields[2]),
        system: number(fields[3]) + number(fields[6]) + number(fields[7]),
        idle: number(fields[4]),
        wait: number(fields[5]),
        total: 0,
    };
    for value in &fields[1..9] {
        cpu.total = cpu.total.saturating_add(number(value));
    }
    details.cpus.push(cpu);
}

fn parse_network(line: &str, details: &mut MonitorDetails) {
    let Some((name, rest)) = line.split_once(':') else {
        return;
    };
    if details.networks.len() >= MAX_INTERFACES {
        return;
    }
    let fields: Vec<&str> = rest.split_whitespace().collect();
    if fields.len() >= 16 {
        details.networks.push(NetworkCounter {
            name: name.trim().to_owned(),
            received: number(fields[0]),
            sent: number(fields[8]),
        });
    }
}

fn parse_disk(fields: &[&str], details: &mut MonitorDetails) {
    if fields.len() < 14 || details.disks.len() >= MAX_DISKS {
        return;
    }
    if fields[2].starts_with("loop") || fields[2].starts_with("ram") {
        return;
    }
    details.disks.push(DiskCounter {
        name: fields[2].to_owned(),
        reads: number(fields[3]),
        writes: number(fields[7]),
    });
}

fn parse_partition(fields: &[&str], details: &mut MonitorDetails) {
    if fields.len() < 6 || !fields[0].starts_with('/') || details.partitions.len() >= MAX_PARTITIONS
    {
        return;
    }
    details.partitions.push(Partition {
        device: fields[0].to_owned(),
        mount: fields[5..].join(" "),
        total: number(fields[1]).saturating_mul(1024),
        used: number(fields[2]).saturating_mul(1024),
        percent: decimal(fields[4].trim_end_matches('%')),
    });
}

fn parse_process(fields: &[&str], details: &mut MonitorDetails) {
    if fields.len() < 7 || details.processes.len() >= MAX_PROCESSES {
        return;
    }
    let Ok(pid) = fields[0].parse::<i32>() else {
        return;
    };
    if pid <= 0 {
        return;
    }
    details.processes.push(ProcessMetric {
        pid,
        user: fields[1].to_owned(),
        name: fields[6..].join(" "),
        cpu: decimal(fields[2]),
        memory: decimal(fields[3]),
        rss: number(fields[4]).saturating_mul(1024),
        start: number(fields[5]),
    });
}

fn parse_gpu(line: &str, details: &mut MonitorDetails) {
    if details.gpus.len() >= MAX_GPUS {
        return;
    }
    let fields: Vec<&str> = line.split(',').collect();
    if fields.len() != 5 {
        return;
    }
    details.gpus.push(Gpu {
        name: fields[0].trim().to_owned(),
        used: number(fields[2].trim()).saturating_mul(1 << 20),
        total: number(fields[3].trim()).saturating_mul(1 << 20),
        utilization: decimal(fields[1]),
        temperature: decimal(fields[4]),
    });
}

fn parse_sockets(fields: &[&str], details: &mut MonitorDetails) {
    if fields.len() <= 2 {
        return;
    }
    let prefix = fields[0];
    let mut index = 1;
    while index + 1 < fields.len() {
        let key = format!("{prefix}{}", fields[index]);
        let value = number(fields[index + 1]);
        match key.as_str() {
            "sockets:used" => details.connections = value,
            "TCP:inuse" => details.tcp_connections = value,
            "TCP:tw" => details.time_wait = value,
            "UDP:inuse" => details.udp_connections = value,
            _ => {}
        }
        index += 2;
    }
}

fn parse_handles(fields: &[&str], details: &mut MonitorDetails) {
    if fields.len() == 3 {
        details.handles = number(fields[0]);
        details.handle_limit = number(fields[2]);
    }
}

fn parse_listener(fields: &[&str], details: &mut MonitorDetails) {
    if fields.len() >= 5 && details.listeners.len() < MAX_LISTENERS {
        details.listeners.push(Listener {
            protocol: fields[0].to_owned(),
            address: fields[4].to_owned(),
        });
    }
}

fn parse_address(fields: &[&str], details: &mut MonitorDetails) {
    if fields.len() >= 4 && details.addresses.len() < MAX_ADDRESSES {
        details.addresses.insert(
            fields[1].trim_end_matches(':').to_owned(),
            fields[3].to_owned(),
        );
    }
}

/// Parses an unsigned integer, treating malformed values as zero.
pub(crate) fn number(value: &str) -> u64 {
    value.parse::<u64>().unwrap_or(0)
}

/// Parses a non-negative finite decimal, treating malformed values as zero.
pub(crate) fn decimal(value: &str) -> f64 {
    let parsed = value.trim().parse::<f64>().unwrap_or(0.0);
    if parsed < 0.0 || !parsed.is_finite() || parsed > 1e15 {
        0.0
    } else {
        parsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "Linux\n__LOAD__\n0.1 0.2 0.3 1/10 42\n__MEM__\nMemTotal: 1000 kB\nMemAvailable: 400 kB\n__UPTIME__\n123.0 50.0\n__DISK__\nFilesystem Blocks Used Available Capacity Mounted\n/dev/root 100 20 80 20% /\n";

    const DETAILED: &str = r"@KERNEL
Linux 6.8.0
@CPU
cpu 100 0 50 800 25 0 25 0 0 0
cpu0 50 0 25 400 12 0 13 0
@NET
 eth0: 1000 0 0 0 0 0 0 0 2000 0 0 0 0 0 0 0
@IO
 8 0 vda 100 0 0 0 200 0 0 0 0 0 0
@PART
/dev/vda 1000 250 750 25% /
@PROC
42 user 20.5 10 1024 999 worker name
@GPU
Example GPU, 30, 512, 1024, 45
@SOCKET
sockets: used 50
TCP: inuse 20 orphan 0 tw 5 alloc 25 mem 2
UDP: inuse 3 mem 2
@HANDLES
100 0 10000
@PORT
tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:*
@ADDR
2: eth0 inet 192.0.2.1/24 brd 192.0.2.255
@TEMP
45000
";

    #[test]
    fn parses_the_basic_linux_snapshot() {
        let metrics = parse_metrics(FIXTURE).unwrap();
        assert_eq!(metrics.memory_total, 1_024_000);
        assert_eq!(metrics.memory_used, 614_400);
        assert_eq!(metrics.load, "0.1 0.2 0.3 1/10 42");
        assert_eq!(metrics.uptime, "123.0 50.0");
        assert_eq!(metrics.disk.lines().count(), 2);
    }

    #[test]
    fn rejects_malformed_or_non_linux_responses() {
        assert!(parse_metrics("bad").is_err());
        assert!(parse_metrics(&FIXTURE.replace("Linux", "Darwin")).is_err());
        assert!(parse_metrics(&FIXTURE.replace("400 kB", "2000 kB")).is_err());
        assert!(parse_metrics(&FIXTURE.replacen("MemAvailable: 400 kB\n", "", 1)).is_err());
    }

    #[test]
    fn details_are_optional_and_bounded() {
        let metrics = parse_metrics(&format!("{FIXTURE}__DETAILS__\n{DETAILED}")).unwrap();
        let details = metrics.details;
        assert_eq!(details.cpus.len(), 2);
        assert_eq!(details.cpus[0].total, 1000);
        assert_eq!(details.networks.len(), 1);
        assert_eq!(details.gpus.len(), 1);
        assert_eq!(details.gpus[0].used, 512 << 20);
        assert_eq!(details.processes[0].start, 999);
        assert_eq!(details.connections, 50);
        assert_eq!(details.time_wait, 5);
        assert_eq!(details.listeners.len(), 1);
        assert_eq!(
            details.addresses.get("eth0").map(String::as_str),
            Some("192.0.2.1/24")
        );
        assert_eq!(details.temperature, 45.0);

        let degraded = parse_details(
            "@GPU\nN/A\n@CPU\nmalformed\n",
            "SwapTotal: 100 kB\nSwapFree: 25 kB\nCached: 4 kB",
        );
        assert!(degraded.gpus.is_empty());
        assert!(degraded.cpus.is_empty());
        assert_eq!(degraded.swap_used, 75 * 1024);
        assert_eq!(degraded.cached, 4 * 1024);
    }

    #[test]
    fn cpu_listing_stops_at_the_core_budget() {
        let huge = "cpu0 1 1 1 1 1 1 1 1\n".repeat(1000);
        let details = parse_details(&format!("@CPU\n{huge}"), "");
        assert_eq!(details.cpus.len(), MAX_CPUS);
    }

    #[test]
    fn oversized_responses_are_refused() {
        let raw = "x".repeat(MAX_MONITOR_OUTPUT + 1);
        assert!(matches!(
            parse_metrics(&raw),
            Err(TerminalError::MonitorOutputTooLarge)
        ));
    }
}
