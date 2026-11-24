//! Shared presentation for local seconds and Linux /proc/uptime.

pub(super) fn parse_proc_uptime(raw: &str) -> String {
    // The second field is cumulative idle CPU time, not part of uptime.
    let seconds = raw
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<f64>().ok());
    match seconds {
        Some(s) if s.is_finite() && s >= 0. && s < u64::MAX as f64 => format_uptime(s as u64),
        _ => "—".into(),
    }
}

pub(super) fn format_uptime(seconds: u64) -> String {
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
    fn linux_uptime_uses_only_elapsed_seconds_and_discards_fractions() {
        assert_eq!(parse_proc_uptime("86031.39 480622.73\n"), "23小时 53分钟");
        assert_eq!(parse_proc_uptime("90000.99 999999999.0"), "1天 1小时");
        assert_eq!(parse_proc_uptime("59.99 0.0"), "0分钟");
        assert_eq!(parse_proc_uptime("60.0 0.0"), "1分钟");
        assert_eq!(parse_proc_uptime("3600.0 0.0"), "1小时 0分钟");
    }

    #[test]
    fn invalid_uptime_is_unavailable_without_misrepresenting_it_as_zero() {
        for raw in ["", "bad 480622.73", "NaN 0", "inf 0", "-1 0", "1e100 0"] {
            assert_eq!(parse_proc_uptime(raw), "—", "{raw}");
        }
    }
}
