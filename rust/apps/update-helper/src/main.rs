use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use superlink_update::{ApplyRequest, UpdateError};

fn run() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("usage: update-helper REQUEST PARENT_PID DATA_ROOT".into());
    }
    let path = PathBuf::from(&args[0]);
    if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 64 << 10 {
        return Err("更新请求超过大小限制".into());
    }
    let mut request: ApplyRequest =
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    superlink_update::validate_request(&request).map_err(|e| e.to_string())?;
    let pid = args[1]
        .to_str()
        .ok_or("父进程编号无效")?
        .parse::<usize>()
        .map_err(|_| "父进程编号无效")?;
    let root = std::fs::canonicalize(PathBuf::from(&args[2])).map_err(|e| e.to_string())?;
    if !path.starts_with(root.join("updates")) {
        return Err("更新请求不在当前工作区".into());
    }
    let mut system = sysinfo::System::new();
    let parent = sysinfo::Pid::from(pid);
    let started = Instant::now();
    loop {
        if system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[parent])) == 0 {
            break;
        }
        match system.process(parent) {
            None => break,
            Some(process) => {
                if process
                    .exe()
                    .is_none_or(|exe| !exe.starts_with(&request.target))
                {
                    return Err("父进程不属于更新目标".into());
                }
            }
        }
        if started.elapsed() > Duration::from_secs(120) {
            return Err("等待应用退出超时，未替换安装包".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let state = root.join("state.db");
    if state.is_file() {
        let connection = rusqlite::Connection::open(&state).map_err(|e| e.to_string())?;
        let busy: i64 = connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if busy != 0 {
            return Err("工作区仍被其他进程占用，更新已停止".into());
        }
        drop(connection);
        let backup = root
            .join("updates")
            .join(format!("state-{}.db", request.token));
        request = request.with_state(&state, backup);
    }
    let report = superlink_update::apply_staged_with(&request, |request| {
        let package = superlink_update::read_package(&request.target)?;
        let executable = request.target.join(package.executable);
        let mut child = std::process::Command::new(executable)
            .spawn()
            .map_err(|e| UpdateError::Request(format!("启动更新应用失败：{e}")))?;
        // Catch immediate launch failures while retaining the previous bundle for rollback.
        std::thread::sleep(Duration::from_secs(3));
        if child
            .try_wait()
            .map_err(|e| UpdateError::Request(e.to_string()))?
            .is_some()
        {
            return Err(UpdateError::Request("新版本启动后提前退出".into()));
        }
        Ok(())
    });
    report
        .write(&root.join("updates/last-update.json"))
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(path);
    if !report.success {
        return Err(report.message);
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("SuperLink 更新未完成：{error}");
        std::process::exit(1);
    }
}
