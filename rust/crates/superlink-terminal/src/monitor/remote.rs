//! Remote monitoring over one-shot SSH exec channels.

use std::sync::Arc;
use std::time::Duration;

use russh::ChannelMsg;
use russh::client::Handle;
use tokio::time::timeout;

use crate::error::{Result, TerminalError};
use crate::monitor::commands::{
    DETAILED_MONITOR_COMMAND, MONITOR_COMMAND, large_files_command, stop_process_command,
};
use crate::monitor::{MAX_MONITOR_OUTPUT, Metrics, ProcessMetric, parse_metrics};
use crate::ssh::{ClientHandler, SshClient};
use crate::transfer::CancelToken;

const MONITOR_TIMEOUT: Duration = Duration::from_secs(8);
const SCAN_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_TIMEOUT: Duration = Duration::from_secs(8);

/// One entry of a largest-files scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeFile {
    pub path: String,
    pub size: u64,
}

impl SshClient {
    /// Reads a bounded Linux health snapshot. Optional sections degrade
    /// individually; only the basic snapshot can fail the call.
    pub fn monitor(&self, cancel: &CancelToken) -> Result<Metrics> {
        let command = format!("{MONITOR_COMMAND}{DETAILED_MONITOR_COMMAND}");
        let raw = self.run_command(&command, MONITOR_TIMEOUT, cancel)?;
        parse_metrics(&raw)
    }

    /// Finds the largest files below the given absolute directories.
    pub fn large_files(
        &self,
        paths: &[String],
        minimum: u64,
        count: usize,
        cancel: &CancelToken,
    ) -> Result<Vec<LargeFile>> {
        let command = large_files_command(paths, minimum, count)?;
        let raw = self.run_command(&command, SCAN_TIMEOUT, cancel)?;
        Ok(parse_large_files(&raw, count))
    }

    /// Sends SIGTERM only when the process start ticks still match.
    pub fn stop_process(&self, process: &ProcessMetric, cancel: &CancelToken) -> Result<()> {
        let command = stop_process_command(process)?;
        self.run_command(&command, STOP_TIMEOUT, cancel).map(|_| ())
    }

    fn run_command(&self, command: &str, budget: Duration, cancel: &CancelToken) -> Result<String> {
        let runtime = crate::runtime::runtime()?;
        let handle = Arc::clone(self.handle());
        runtime.block_on(async move {
            match timeout(budget, exec(&handle, command, cancel)).await {
                Ok(result) => result,
                Err(_) => Err(TerminalError::Timeout),
            }
        })
    }
}

fn parse_large_files(raw: &str, count: usize) -> Vec<LargeFile> {
    let mut files = Vec::new();
    for entry in raw.split('\0') {
        if files.len() >= count {
            break;
        }
        let Some((size, path)) = entry.split_once('\t') else {
            continue;
        };
        files.push(LargeFile {
            path: path.to_owned(),
            size: size.parse::<u64>().unwrap_or(0),
        });
    }
    files
}

fn push_bounded(output: &mut Vec<u8>, data: &[u8]) -> Result<()> {
    if output.len() + data.len() > MAX_MONITOR_OUTPUT {
        return Err(TerminalError::MonitorOutputTooLarge);
    }
    output.extend_from_slice(data);
    Ok(())
}

/// Runs a probe command, ignoring stderr and bounding captured stdout.
async fn exec(
    handle: &Arc<Handle<ClientHandler>>,
    command: &str,
    cancel: &CancelToken,
) -> Result<String> {
    let channel = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(TerminalError::Cancelled),
        result = handle.channel_open_session() => result.map_err(TerminalError::channel)?,
    };
    let (mut reader, writer) = channel.split();
    writer
        .exec(true, command)
        .await
        .map_err(TerminalError::channel)?;
    let mut output = Vec::new();
    // A rejected exec must fail instead of reading an empty channel until timeout.
    loop {
        let message = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(TerminalError::Cancelled),
            message = reader.wait() => message,
        };
        match message {
            Some(ChannelMsg::Success) => break,
            Some(ChannelMsg::Failure) => {
                return Err(TerminalError::Channel("远程服务器拒绝了监控命令".into()));
            }
            Some(ChannelMsg::Data { data }) => push_bounded(&mut output, &data)?,
            Some(ChannelMsg::ExtendedData { .. }) => {}
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                return Err(TerminalError::Channel("远程通道在命令启动前关闭".into()));
            }
            _ => {}
        }
    }
    let mut status = None;
    loop {
        let message = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(TerminalError::Cancelled),
            message = reader.wait() => message,
        };
        match message {
            Some(ChannelMsg::Data { data }) => push_bounded(&mut output, &data)?,
            Some(ChannelMsg::ExtendedData { .. }) => {}
            Some(ChannelMsg::ExitStatus { exit_status }) => status = Some(exit_status),
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
            _ => {}
        }
    }
    if let Some(status) = status
        && status != 0
    {
        return Err(TerminalError::Other(format!(
            "远程命令失败，退出状态 {status}"
        )));
    }
    String::from_utf8(output).map_err(|_| TerminalError::MonitorFormat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn largest_files_are_parsed_and_bounded() {
        let raw = "10\t/a\u{0}5\t/b\u{0}1\t/c\u{0}";
        assert_eq!(
            parse_large_files(raw, 2),
            vec![
                LargeFile {
                    path: "/a".into(),
                    size: 10
                },
                LargeFile {
                    path: "/b".into(),
                    size: 5
                },
            ]
        );
        assert_eq!(parse_large_files(raw, 10).len(), 3);
    }

    #[test]
    fn malformed_scan_entries_are_skipped() {
        assert!(parse_large_files("\0junk\0", 10).is_empty());
    }
}
