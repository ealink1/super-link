//! Rust-only, bounded JSON-lines driver host. Stdout is exclusively protocol data.
mod catalog;
mod catalog_constraints;
mod catalog_ddl;
mod catalog_objects;
mod connection;
mod dameng;
mod duckdb;
mod elasticsearch;
mod gauss;
mod http;
mod iotdb;
mod iris;
mod iris_codec;
mod kafka;
mod mongo;
mod mqtt;
mod navicat;
mod navicat_codec;
mod network;
mod numeric;
mod oracle;
mod protocol;
mod pulsar;
mod redis;
mod rocketmq;
mod rocketmq_decode;
mod rows;
mod runtime;
mod services;
mod sql;
mod sql_catalog;
mod streaming;
mod tls;

mod sqlserver;
mod vectors;

use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Read, Write},
    sync::{Arc, Mutex, mpsc},
};
use superlink_infra::QueryCancel;

const MAX_FRAME: usize = 32 << 20;
fn main() {
    if serve().is_err() {
        std::process::exit(1);
    }
}
fn serve() -> io::Result<()> {
    let kind = driver_kind()?;
    let output = Arc::new(Mutex::new(io::BufWriter::new(io::stdout())));
    let cancellation = QueryCancel::new();
    let (sender, requests) = mpsc::sync_channel::<Value>(1);
    let writer = output.clone();
    let cancel = cancellation.clone();
    let worker = std::thread::spawn(move || {
        let mut runtime = runtime::Runtime::new(kind, cancel);
        while let Ok(request) = requests.recv() {
            let id = request.get("id").cloned().unwrap_or(Value::Null);
            let response = match runtime.handle(&request, &writer) {
                Ok(mut response) => {
                    response["id"] = id;
                    response["success"] = json!(true);
                    response
                }
                Err(error) => json!({"id":id,"success":false,"error":error}),
            };
            if write_frame(&writer, &response).is_err() {
                break;
            }
        }
    });
    let mut input = io::stdin().lock();
    loop {
        let mut line = Vec::new();
        let count = input
            .by_ref()
            .take((MAX_FRAME + 1) as u64)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            break;
        }
        if count > MAX_FRAME {
            cancellation.cancel();
            break;
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let request: Value = match serde_json::from_slice(&line) {
            Ok(value) => value,
            Err(_) => {
                write_frame(
                    &output,
                    &json!({"id":null,"success":false,"error":"无效请求"}),
                )?;
                continue;
            }
        };
        if request["method"] == "cancelQuery" {
            cancellation.cancel();
            continue;
        }
        if let Err(mpsc::TrySendError::Full(request)) = sender.try_send(request) {
            write_frame(
                &output,
                &json!({"id":request["id"],"success":false,"error":"驱动请求队列已满"}),
            )?;
        }
    }
    drop(sender);
    let _ = worker.join();
    Ok(())
}
fn driver_kind() -> io::Result<String> {
    let args: Vec<_> = std::env::args().collect();
    let kind = if args.get(1).map(String::as_str) == Some("--driver") {
        args.get(2).cloned().unwrap_or_default()
    } else {
        std::path::Path::new(&args[0])
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .split("-driver-agent")
            .next()
            .unwrap_or_default()
            .to_string()
    };
    superlink_domain::resolve(&kind)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "驱动类型无效"))?;
    Ok(kind)
}
pub(crate) type Output = Arc<Mutex<io::BufWriter<io::Stdout>>>;
pub(crate) fn write_frame(output: &Output, value: &Value) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "响应帧超限"));
    }
    bytes.push(b'\n');
    let mut output = output.lock().map_err(|_| io::Error::other("输出已关闭"))?;
    output.write_all(&bytes)?;
    output.flush()
}
