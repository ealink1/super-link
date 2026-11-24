use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};
struct Agent {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    sequence: u64,
}
impl Agent {
    fn new(kind: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_superlink-driver-agent"))
            .args(["--driver", kind])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
            sequence: 0,
        }
    }
    fn send(&mut self, mut request: Value) {
        self.sequence += 1;
        request["id"] = json!(self.sequence);
        writeln!(self.input, "{request}").unwrap();
        self.input.flush().unwrap();
    }
    fn frame(&mut self) -> Value {
        let mut line = String::new();
        assert!(
            self.output.read_line(&mut line).unwrap() > 0,
            "agent closed stdout"
        );
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["id"], self.sequence);
        value
    }
    fn call(&mut self, request: Value) -> Value {
        self.send(request);
        let value = self.frame();
        assert_eq!(value["success"], true, "{value}");
        value
    }
    fn connect(&mut self, kind: &str) {
        self.call(json!({"method":"connect","config":{"type":kind,"database":":memory:"}}));
    }
}
impl Drop for Agent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[test]
fn every_data_source_has_rust_protocol_identity() {
    let kinds = [
        "mysql",
        "postgres",
        "oracle",
        "goldendb",
        "custom",
        "redis",
        "nacos",
        "chroma",
        "qdrant",
        "milvus",
        "rocketmq",
        "mqtt",
        "kafka",
        "rabbitmq",
        "pulsar",
        "mariadb",
        "oceanbase",
        "diros",
        "starrocks",
        "sphinx",
        "sqlserver",
        "sqlite",
        "duckdb",
        "dameng",
        "kingbase",
        "highgo",
        "vastbase",
        "opengauss",
        "gaussdb",
        "iris",
        "cache",
        "mongodb",
        "tdengine",
        "iotdb",
        "clickhouse",
        "elasticsearch",
        "trino",
    ];
    assert_eq!(kinds.len(), 37);
    for kind in kinds {
        let mut agent = Agent::new(kind);
        let value = agent.call(json!({"method":"metadata"}));
        assert_eq!(value["data"]["driverType"], kind);
        assert_eq!(value["data"]["implementation"], "rust");
        assert_eq!(value["data"]["protocolSchema"], "json-lines-v3-rust");
    }
}
#[test]
fn sqlite_bound_values_transaction_catalog_and_stream_share_one_connection() {
    let mut agent = Agent::new("sqlite");
    agent.connect("sqlite");
    agent.call(json!({"method":"openSession"}));
    agent.call(json!({"method":"exec","query":"CREATE TABLE t(id INTEGER PRIMARY KEY,b BLOB)"}));
    agent.call(json!({"method":"exec","query":"BEGIN"}));
    agent.call(json!({"method":"exec","query":"INSERT INTO t VALUES(?1,?2)","args":["9223372036854775807","AP8="],"argTypes":["integer","binary"]}));
    let value = agent.call(json!({"method":"query","query":"SELECT id,hex(b) FROM t"}));
    assert_eq!(value["data"][0], json!([i64::MAX, "00FF"]));
    agent.call(json!({"method":"exec","query":"ROLLBACK"}));
    assert_eq!(
        agent.call(json!({"method":"query","query":"SELECT count(*) FROM t"}))["data"][0][0],
        0
    );
    assert!(agent.call(json!({"method":"getColumns","tableName":"t"}))["data"][0]["key"] == "PRI");
    assert!(
        agent.call(json!({"method":"getCreateStatement","tableName":"t"}))["data"]
            .as_str()
            .unwrap()
            .contains("CREATE TABLE")
    );
    agent.send(json!({"method":"streamQuery","query":"WITH RECURSIVE n(x) AS(VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<10000) SELECT x,printf('%04096d',x) FROM n"}));
    let mut rows = 0;
    loop {
        let frame = agent.frame();
        assert_eq!(frame["success"], true, "{frame}");
        match frame["chunkType"].as_str().unwrap() {
            "rows" => {
                let chunk = frame["data"].as_array().unwrap();
                assert!(chunk.len() <= 256);
                rows += chunk.len();
            }
            "done" => break,
            "columns" => assert_eq!(frame["fields"].as_array().unwrap().len(), 2),
            _ => panic!("unknown chunk"),
        }
    }
    assert_eq!(rows, 10000);
}
#[test]
#[cfg(not(all(target_os = "windows", target_arch = "aarch64")))]
fn duckdb_table_structure_and_exact_decimal_work_without_external_services() {
    let mut agent = Agent::new("duckdb");
    agent.connect("duckdb");
    agent.call(
        json!({"method":"exec","query":"CREATE TABLE t(id BIGINT PRIMARY KEY,n DECIMAL(38,18))"}),
    );
    agent.call(json!({"method":"exec","query":"INSERT INTO t VALUES(9223372036854775807,12345678901234567890.123456789012345678)"}));
    let value = agent.call(json!({"method":"query","query":"SELECT id,CAST(n AS VARCHAR) FROM t"}));
    assert_eq!(value["data"][0][0], json!(i64::MAX));
    assert_eq!(
        value["data"][0][1],
        "12345678901234567890.123456789012345678"
    );
    let value = agent.call(json!({"method":"getColumns","tableName":"t"}));
    assert_eq!(value["data"][0]["key"], "PRI");
    let value =
        agent.call(json!({"method":"getColumns","dbName":"memory","tableName":"memory.main.t"}));
    assert_eq!(value["data"][0]["key"], "PRI");
    agent.call(json!({"method":"exec","query":"CREATE TABLE \"a.b\"(id INTEGER PRIMARY KEY)"}));
    let value = agent
        .call(json!({"method":"getColumns","dbName":"memory","tableName":"memory.main.\"a.b\""}));
    assert_eq!(value["data"][0]["name"], "id");
    assert!(agent.call(json!({"method":"getCreateStatement","tableName":"memory.main.\"a.b\""}))["data"].as_str().unwrap().contains("a.b"));
}
#[test]
fn rejects_unknown_bound_types_and_never_bypasses_unsupported_routes() {
    let mut agent = Agent::new("sqlite");
    agent.connect("sqlite");
    agent.send(json!({"method":"query","query":"SELECT ?1","args":["x"],"argTypes":["unknown"]}));
    assert_eq!(agent.frame()["success"], false);
    let mut agent = Agent::new("kafka");
    agent.send(json!({"method":"connect","config":{"type":"kafka","host":"127.0.0.1","port":1,"useSSH":true,"ssh":{}}}));
    let value = agent.frame();
    assert_eq!(value["success"], false);
    assert!(value["error"].as_str().unwrap().contains("动态节点"));
}
#[test]
fn request_deadline_interrupts_an_in_flight_sqlite_query() {
    let mut agent = Agent::new("sqlite");
    agent.connect("sqlite");
    let start = std::time::Instant::now();
    agent.send(json!({"method":"query","timeoutMs":20,"query":"WITH RECURSIVE n(x) AS(VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT sum(x) FROM n"}));
    assert_eq!(agent.frame()["success"], false);
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
}
