//! Fixed remote probe commands.
//!
//! Probes never interpolate user text except through strict validation and
//! POSIX single-quote escaping, so a host field can never become a command.

use crate::error::{Result, TerminalError};
use crate::monitor::ProcessMetric;

/// Basic Linux snapshot. The command is read-only and fixed.
pub const MONITOR_COMMAND: &str = r#"LC_ALL=C; export LC_ALL; uname -s; printf '\n__LOAD__\n'; cat /proc/loadavg 2>/dev/null; printf '\n__MEM__\n'; cat /proc/meminfo 2>/dev/null; printf '\n__UPTIME__\n'; cat /proc/uptime 2>/dev/null; printf '\n__DISK__\n'; df -P / 2>/dev/null"#;

/// Optional sections appended to the basic snapshot.
pub const DETAILED_MONITOR_COMMAND: &str = r#"; printf '\n__DETAILS__\n'; printf '@KERNEL\n'; uname -snr; printf '@CPU\n'; cat /proc/stat; printf '@NET\n'; cat /proc/net/dev; printf '@IO\n'; cat /proc/diskstats; printf '@PART\n'; df -Pk -x tmpfs -x devtmpfs -x overlay -x squashfs; printf '@PROC\n'; { ps -eo pid=,user=,pcpu=,pmem=,rss=,comm= --sort=-pcpu | head -20; ps -eo pid=,user=,pcpu=,pmem=,rss=,comm= --sort=-rss | head -20; } | awk '!seen[$1]++' | while read pid user cpu mem rss name; do start=$(awk '{sub(/^.*\) /,"");print $20}' /proc/$pid/stat 2>/dev/null); printf '%s %s %s %s %s %s %s\n' "$pid" "$user" "$cpu" "$mem" "$rss" "$start" "$name"; done; printf '@GPU\n'; if command -v nvidia-smi >/dev/null 2>&1; then nvidia-smi --query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu --format=csv,noheader,nounits 2>/dev/null | head -16; fi; printf '@SOCKET\n'; cat /proc/net/sockstat; printf '@HANDLES\n'; cat /proc/sys/fs/file-nr; printf '@PORT\n'; ss -H -lntu 2>/dev/null | head -100; printf '@ADDR\n'; ip -o -4 addr show 2>/dev/null | head -128; printf '@TEMP\n'; for f in /sys/class/thermal/thermal_zone*/temp; do cat "$f" 2>/dev/null; done; true"#;

/// Shortest scan size in bytes (1 MiB).
const MINIMUM_SCAN_BYTES: u64 = 1 << 20;
/// Longest scan size in bytes (1 PiB).
const MAXIMUM_SCAN_BYTES: u64 = 1 << 50;
const MAXIMUM_SCAN_PATHS: usize = 8;
const MAXIMUM_SCAN_RESULTS: usize = 100;

/// Builds a bounded find pipeline for the largest files below each directory.
pub fn large_files_command(paths: &[String], minimum: u64, count: usize) -> Result<String> {
    if paths.is_empty()
        || paths.len() > MAXIMUM_SCAN_PATHS
        || !(MINIMUM_SCAN_BYTES..=MAXIMUM_SCAN_BYTES).contains(&minimum)
        || !(1..=MAXIMUM_SCAN_RESULTS).contains(&count)
    {
        return Err(TerminalError::Validation("扫描参数无效".into()));
    }
    let mut quoted = Vec::with_capacity(paths.len());
    for path in paths {
        if !path.starts_with('/') || path.contains(['\0', '\r', '\n']) || path.len() > 4096 {
            return Err(TerminalError::Validation("请输入绝对目录路径".into()));
        }
        quoted.push(shell_quote(path));
    }
    let command = format!(
        "LC_ALL=C find {} -xdev -type f -size +{}c -printf '%s\\t%p\\0' 2>/dev/null | sort -z -nr | head -z -n {}",
        quoted.join(" "),
        minimum - 1,
        count
    );
    Ok(format!("timeout 25s sh -c {}", shell_quote(&command)))
}

/// Builds a command that only signals a process whose kernel start ticks
/// still match, so PID reuse cannot hit an unrelated process.
pub fn stop_process_command(process: &ProcessMetric) -> Result<String> {
    if process.pid <= 1 || process.start == 0 {
        return Err(TerminalError::Validation(
            "无法安全识别此进程，请刷新后重试".into(),
        ));
    }
    let pid = process.pid;
    let start = process.start;
    Ok(format!(
        "test \"$(awk '{{sub(/^.*\\) /,\"\");print $20}}' /proc/{pid}/stat 2>/dev/null)\" = {start} && kill -TERM -- {pid}"
    ))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn scan_parameters_are_strictly_validated() {
        for paths in [
            Vec::new(),
            vec!["relative".to_owned()],
            vec!["/tmp\nattack".to_owned()],
            vec![format!("/{}", "a".repeat(5000))],
        ] {
            assert!(large_files_command(&paths, 100 << 20, 20).is_err());
        }
        let too_many: Vec<String> = (0..9).map(|index| format!("/tmp/{index}")).collect();
        assert!(large_files_command(&too_many, 100 << 20, 20).is_err());
        assert!(large_files_command(&["/tmp".to_owned()], 1, 20).is_err());
        assert!(large_files_command(&["/tmp".to_owned()], 100 << 20, 0).is_err());
    }

    #[test]
    fn scan_quoting_survives_shell_metacharacters() {
        let hostile = "/tmp/o'brien; $(echo no)";
        let command = large_files_command(&[hostile.to_owned()], 100 << 20, 20).unwrap();
        assert!(command.starts_with("timeout 25s sh -c "));
        assert!(command.contains("$(echo no)"));
        // The quoted text must round-trip through a shell unchanged.
        let output = Command::new("sh")
            .arg("-c")
            .arg(format!("printf '%s' {}", shell_quote(hostile)))
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout), hostile);
    }

    #[test]
    fn stop_requires_a_verified_start_time() {
        for process in [
            ProcessMetric {
                pid: 1,
                start: 4,
                ..ProcessMetric::default()
            },
            ProcessMetric {
                pid: 4,
                start: 0,
                ..ProcessMetric::default()
            },
        ] {
            assert!(stop_process_command(&process).is_err());
        }
        let command = stop_process_command(&ProcessMetric {
            pid: 42,
            start: 999,
            ..ProcessMetric::default()
        })
        .unwrap();
        assert!(command.contains("kill -TERM -- 42"));
        assert!(command.contains("= 999"));
    }

    #[test]
    fn probe_commands_are_valid_shell_syntax() {
        let Some(shell) = find_shell() else {
            return;
        };
        let scan = large_files_command(&["/tmp".to_owned()], 100 << 20, 20).unwrap();
        let stop = stop_process_command(&ProcessMetric {
            pid: 42,
            start: 999,
            ..ProcessMetric::default()
        })
        .unwrap();
        for command in [
            scan,
            stop,
            format!("{MONITOR_COMMAND}{DETAILED_MONITOR_COMMAND}"),
        ] {
            let output = Command::new(shell)
                .arg("-n")
                .arg("-c")
                .arg(&command)
                .output();
            let Ok(output) = output else { continue };
            assert!(
                output.status.success(),
                "invalid probe syntax: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    fn find_shell() -> Option<&'static str> {
        ["/bin/sh", "/usr/bin/sh"]
            .into_iter()
            .find(|candidate| std::path::Path::new(candidate).exists())
    }
}
