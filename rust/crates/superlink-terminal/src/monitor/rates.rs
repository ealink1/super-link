//! Counter deltas between two monitor samples.
//!
//! Rates come from cumulative counters so a load average is never mistaken
//! for CPU utilization. Counter resets and disappearing devices yield zero
//! instead of negative or absurd rates.

use crate::monitor::{MonitorDetails, NetworkCounter};

/// Utilization of one CPU between two samples.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuRate {
    pub name: String,
    pub used: f64,
    pub user: f64,
    pub system: f64,
    pub wait: f64,
}

/// Throughput of one network interface, in bytes per second.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetworkRate {
    pub name: String,
    pub received: f64,
    pub sent: f64,
}

/// Throughput of one disk, in operations per second.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiskRate {
    pub name: String,
    pub reads: f64,
    pub writes: f64,
}

/// All rates derived from two consecutive samples.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MonitorRates {
    pub cpus: Vec<CpuRate>,
    pub networks: Vec<NetworkRate>,
    pub disks: Vec<DiskRate>,
    /// False when no valid delta could be computed.
    pub ready: bool,
}

/// Computes rates, returning an empty result for the first ever sample.
pub fn rates(previous: &MonitorDetails, current: &MonitorDetails) -> MonitorRates {
    let mut result = MonitorRates::default();
    let (Some(previous_at), Some(current_at)) = (previous.sampled, current.sampled) else {
        return result;
    };
    let seconds = current_at
        .checked_duration_since(previous_at)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or(0.0);
    if seconds <= 0.0 {
        return result;
    }
    result.ready = true;
    for cpu in &current.cpus {
        let Some(previous_cpu) = previous
            .cpus
            .iter()
            .find(|candidate| candidate.name == cpu.name)
        else {
            continue;
        };
        if cpu.total <= previous_cpu.total || cpu.idle < previous_cpu.idle {
            continue;
        }
        let total = (cpu.total - previous_cpu.total) as f64;
        let idle = (cpu.idle - previous_cpu.idle) as f64;
        result.cpus.push(CpuRate {
            name: cpu.name.clone(),
            used: (100.0 * (1.0 - idle / total)).min(100.0),
            user: percent_delta(previous_cpu.user, cpu.user, total),
            system: percent_delta(previous_cpu.system, cpu.system, total),
            wait: percent_delta(previous_cpu.wait, cpu.wait, total),
        });
    }
    for network in &current.networks {
        if let Some(previous_network) = previous
            .networks
            .iter()
            .find(|candidate| is_same_interface(candidate, network))
        {
            result.networks.push(NetworkRate {
                name: network.name.clone(),
                received: rate_delta(previous_network.received, network.received, seconds),
                sent: rate_delta(previous_network.sent, network.sent, seconds),
            });
        }
    }
    for disk in &current.disks {
        if let Some(previous_disk) = previous
            .disks
            .iter()
            .find(|candidate| candidate.name == disk.name)
        {
            result.disks.push(DiskRate {
                name: disk.name.clone(),
                reads: rate_delta(previous_disk.reads, disk.reads, seconds),
                writes: rate_delta(previous_disk.writes, disk.writes, seconds),
            });
        }
    }
    result
}

fn is_same_interface(previous: &NetworkCounter, current: &NetworkCounter) -> bool {
    previous.name == current.name
}

fn rate_delta(previous: u64, current: u64, seconds: f64) -> f64 {
    if current < previous {
        return 0.0;
    }
    (current - previous) as f64 / seconds
}

fn percent_delta(previous: u64, current: u64, total: f64) -> f64 {
    if current < previous || total <= 0.0 {
        return 0.0;
    }
    (100.0 * (current - previous) as f64 / total).min(100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::parse::parse_details;
    use std::time::Duration;

    const DETAILED: &str = r"@KERNEL
Linux 6.8.0
@CPU
cpu 100 0 50 800 25 0 25 0 0 0
cpu0 50 0 25 400 12 0 13 0
@NET
 eth0: 1000 0 0 0 0 0 0 0 2000 0 0 0 0 0 0 0
@IO
 8 0 vda 100 0 0 0 200 0 0 0 0 0 0
";

    #[test]
    fn rates_follow_counter_deltas() {
        let mut previous = parse_details(DETAILED, "");
        let mut current = parse_details(DETAILED, "");
        previous.sampled = Some(std::time::Instant::now());
        current.sampled = previous.sampled.map(|at| at + Duration::from_secs(2));
        current.cpus[0].total += 100;
        current.cpus[0].idle += 60;
        current.cpus[0].user += 25;
        current.cpus[0].system += 10;
        current.cpus[0].wait += 5;
        current.networks[0].received += 2000;
        current.networks[0].sent += 1000;
        current.disks[0].reads += 10;
        let result = rates(&previous, &current);
        assert!(result.ready);
        assert_eq!(result.cpus[0].used, 40.0);
        assert_eq!(result.cpus[0].user, 25.0);
        assert_eq!(result.networks[0].received, 1000.0);
        assert_eq!(result.disks[0].reads, 5.0);
    }

    #[test]
    fn counter_resets_do_not_produce_negative_rates() {
        let mut previous = parse_details(DETAILED, "");
        let mut current = parse_details(DETAILED, "");
        previous.sampled = Some(std::time::Instant::now());
        current.sampled = previous.sampled.map(|at| at + Duration::from_secs(1));
        current.networks[0].received = 1;
        let result = rates(&previous, &current);
        assert_eq!(result.networks[0].received, 0.0);
    }

    #[test]
    fn the_first_sample_reports_no_rates() {
        let mut current = parse_details(DETAILED, "");
        current.sampled = Some(std::time::Instant::now());
        assert!(!rates(&MonitorDetails::default(), &current).ready);
        let mut previous = parse_details(DETAILED, "");
        previous.sampled = Some(std::time::Instant::now());
        assert!(!rates(&previous, &MonitorDetails::default()).ready);
    }
}
