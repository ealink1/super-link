//! 进程内 PostgreSQL 驱动。
//!
//! 覆盖 Rust 端当前用到的 query 子集：类型化取值、行预算、耗时统计。
//! TLS 连接（sslmode=require 及以上）暂不支持，调用方应回退到 agent。

use std::time::{Duration, Instant};
mod streaming;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use fallible_iterator::FallibleIterator;
use postgres::types::{FromSql, Type, WrongType};
use postgres::{Client, Config, NoTls, Row};
use postgres_rustls::MakeTlsConnector;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use serde_json::{Value, json};
use superlink_domain::{Column, ConnectionConfig, ResultSet};

use super::structure::{ColumnEntry, ForeignKeyEntry, IndexEntry, TableEntry, TableStructure};

use crate::session::DriverError;

pub struct NativePostgres {
    client: Client,
}

impl NativePostgres {
    pub fn set_cancellation(&self, cancel: &superlink_infra::QueryCancel) {
        let token = self.client.cancel_token();
        cancel.on_cancel(move || {
            let _ = token.cancel_query(NoTls);
        });
    }
    pub fn open(config: &ConnectionConfig) -> Result<Self, DriverError> {
        let host = if config.host.trim().is_empty() {
            "127.0.0.1"
        } else {
            config.host.trim()
        };
        let source = if !config.dsn.is_empty() {
            &config.dsn
        } else {
            &config.uri
        };
        let mut params: Config = if source.is_empty() {
            Config::new()
        } else {
            source
                .parse()
                .map_err(|_| DriverError::Native("PostgreSQL DSN 无效".into()))?
        };
        if source.is_empty() {
            params
                .host(host)
                .port(if config.port == 0 { 5432 } else { config.port });
        }
        if let Some(address) = config
            .extra
            .get("rustConnectAddress")
            .and_then(Value::as_str)
        {
            let address: std::net::SocketAddr = address
                .parse()
                .map_err(|_| DriverError::Native("转发地址无效".into()))?;
            params.hostaddr(address.ip()).port(address.port());
        }
        // 连接串 / 连接参数里的 query 项合并进 libpq 风格选项；未知键忽略。
        for (key, value) in
            super::connection_params::merged_params(&config.uri, &config.connection_params)
        {
            match key.to_ascii_lowercase().as_str() {
                "application_name" | "application name" => {
                    params.application_name(&value);
                }
                "options" => {
                    params.options(&value);
                }
                "connect_timeout" | "connect timeout" => {
                    if let Ok(seconds) = value.parse::<u64>() {
                        params.connect_timeout(Duration::from_secs(seconds));
                    }
                }
                _ => {}
            }
        }
        params.application_name("SuperLink");
        if source.is_empty() && !config.user.trim().is_empty() {
            params.user(config.user.trim());
        }
        if source.is_empty() && !config.password.is_empty() {
            params.password(&config.password);
        }
        if source.is_empty() && !config.database.trim().is_empty() {
            params.dbname(config.database.trim());
        }
        params.connect_timeout(Duration::from_secs(if config.timeout == 0 {
            10
        } else {
            u64::from(config.timeout)
        }));
        let client = match tls_config(config)? {
            Some(tls) => {
                let connector = MakeTlsConnector::new(tokio_rustls::TlsConnector::from(
                    std::sync::Arc::new(tls),
                ));
                params.connect(connector)
            }
            None => params.connect(NoTls),
        }
        .map_err(|err| {
            let text = err.to_string();
            // 握手即断开基本只有两类原因：openGauss/GaussDB 的 sha256 认证，
            // 或服务端要求非标准 SSL 协商，需要独立 Rust Gauss 协议实现。
            let hint = if text.contains("connection closed") || text.contains("communicating") {
                "（服务端在握手阶段断开：openGauss/GaussDB 的 sha256 认证或非标准 SSL 协商不受支持，请改用 agent）"
            } else {
                ""
            };
            DriverError::Native(format!("连接 PostgreSQL 失败：{text}{hint}"))
        })?;
        Ok(Self { client })
    }

    pub fn revision(&self) -> &'static str {
        "native-postgres"
    }

    pub fn query(
        &mut self,
        sql: &str,
        args: &[Value],
        max_rows: u32,
    ) -> Result<ResultSet, DriverError> {
        let started = Instant::now();
        let params = to_params(args)?;
        let limit = if max_rows == 0 { u32::MAX } else { max_rows };

        let statement = self
            .client
            .prepare(sql)
            .map_err(|err| DriverError::Native(format!("语句准备失败：{err}")))?;
        let columns: Vec<Column> = statement
            .columns()
            .iter()
            .map(|column| Column {
                id: column.name().to_string(),
                name: column.name().to_string(),
            })
            .collect();

        let mut rows = self
            .client
            .query_raw(&statement, params.iter().map(|value| value.as_ref()))
            .map_err(|err| DriverError::Native(format!("查询失败：{err}")))?;

        let mut output: Vec<Vec<Value>> = Vec::new();
        let mut truncated = false;
        let mut memory = crate::budget::ResultBudget::default();
        loop {
            match rows.next() {
                Ok(Some(row)) => {
                    if output.len() as u32 >= limit {
                        truncated = true;
                        break;
                    }
                    let mut values = Vec::with_capacity(columns.len());
                    for index in 0..columns.len() {
                        values.push(to_json(&row, index)?);
                    }
                    if !memory.accept(&values) {
                        truncated = true;
                        break;
                    }
                    output.push(values);
                }
                Ok(None) => break,
                Err(err) => return Err(DriverError::Native(format!("读取行失败：{err}"))),
            }
        }
        let affected = rows.rows_affected().unwrap_or(0) as i64;
        drop(rows);

        Ok(ResultSet {
            columns,
            rows: output,
            messages: Vec::new(),
            rows_affected: affected,
            truncated,
            duration_ns: started.elapsed().as_nanos() as u64,
        })
    }
}

/// public 之外的 schema 用 schema.table 限定，避免同名表冲突。
fn qualify(schema: &str, name: &str) -> String {
    if schema == "public" {
        name.to_string()
    } else {
        format!("{schema}.{name}")
    }
}

/// schema.table 拆分；无前缀按 public 处理。
fn split_table(table: &str) -> (String, String) {
    match table.split_once('.') {
        Some((schema, name)) => (schema.to_string(), name.to_string()),
        None => ("public".to_string(), table.to_string()),
    }
}

/// (索引名, 是否唯一, 列名) 按索引名合并，保持列顺序。
fn group_indexes(rows: Vec<(String, bool, String)>) -> Vec<IndexEntry> {
    let mut list: Vec<IndexEntry> = Vec::new();
    for (name, unique, column) in rows {
        match list.iter_mut().find(|entry| entry.name == name) {
            Some(entry) => entry.columns.push(column),
            None => list.push(IndexEntry {
                columns: vec![column],
                name,
                unique,
            }),
        }
    }
    list
}

/// (约束名, 本地列, 引用表, 引用列) 按约束名合并。
fn group_foreign_keys(rows: Vec<(String, String, String, String)>) -> Vec<ForeignKeyEntry> {
    let mut list: Vec<ForeignKeyEntry> = Vec::new();
    for (name, column, referenced_table, referenced_column) in rows {
        match list.iter_mut().find(|entry| entry.name == name) {
            Some(entry) => {
                entry.columns.push(column);
                entry.referenced_columns.push(referenced_column);
            }
            None => list.push(ForeignKeyEntry {
                columns: vec![column],
                name,
                referenced_table,
                referenced_columns: vec![referenced_column],
            }),
        }
    }
    list
}

fn table_kind(raw: &str) -> &'static str {
    if raw.eq_ignore_ascii_case("VIEW") {
        "view"
    } else {
        "table"
    }
}

impl NativePostgres {
    /// 单事务执行多条语句；PG 的 DDL 是事务性的，失败可完整回滚。
    pub fn execute_batch(&mut self, statements: &[String]) -> Result<(), DriverError> {
        let mut transaction = self
            .client
            .transaction()
            .map_err(|err| DriverError::Native(format!("开启事务失败：{err}")))?;
        for statement in statements {
            transaction
                .batch_execute(statement)
                .map_err(|err| DriverError::Native(format!("执行失败：{err}")))?;
        }
        transaction
            .commit()
            .map_err(|err| DriverError::Native(format!("提交失败：{err}")))
    }
}

/// 结构浏览：系统 schema 之外的普通表与视图。
impl NativePostgres {
    /// PG 的跨库查询需要独立连接，这里只返回当前连接的数据库。
    pub fn databases(&mut self) -> Result<Vec<String>, DriverError> {
        let name: String = self
            .client
            .query_one("SELECT current_database()", &[])
            .map_err(|err| DriverError::Native(format!("读取数据库失败：{err}")))?
            .get(0);
        Ok(vec![name])
    }

    pub fn tables(&mut self) -> Result<Vec<TableEntry>, DriverError> {
        let rows = self
            .client
            .query(
                "SELECT table_schema, table_name, table_type
                 FROM information_schema.tables
                 WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
                 ORDER BY table_schema, table_name LIMIT 10000",
                &[],
            )
            .map_err(|err| DriverError::Native(format!("读取表列表失败：{err}")))?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let schema: String = row.get(0);
                let name: String = row.get(1);
                let kind: String = row.get(2);
                TableEntry {
                    name: qualify(&schema, &name),
                    kind: table_kind(&kind).to_string(),
                }
            })
            .collect())
    }

    pub fn structure(&mut self, table: &str) -> Result<TableStructure, DriverError> {
        Ok(TableStructure {
            columns: self.columns(table)?,
            indexes: self.indexes(table)?,
            foreign_keys: self.foreign_keys(table)?,
        })
    }

    /// COMMENT ON COLUMN 的注释，按列名返回（无注释的列不出现或为 None）。
    fn column_comments(
        &mut self,
        schema: &str,
        table: &str,
    ) -> Result<Vec<(String, Option<String>)>, DriverError> {
        let rows = self
            .client
            .query(
                "SELECT a.attname, d.description
                 FROM pg_class c
                 JOIN pg_namespace n ON n.oid = c.relnamespace
                 JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
                 LEFT JOIN pg_description d ON d.objoid = c.oid AND d.objsubid = a.attnum
                 WHERE n.nspname = $1 AND c.relname = $2 LIMIT 4096",
                &[&schema, &table],
            )
            .map_err(|err| DriverError::Native(format!("读取列注释失败：{err}")))?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, Option<String>>(1)))
            .collect())
    }

    /// pg_index + unnest(indkey)：表达式索引（attnum = 0）会被内连接过滤掉。
    fn indexes(&mut self, table: &str) -> Result<Vec<IndexEntry>, DriverError> {
        let (schema, name) = split_table(table);
        let rows = self
            .client
            .query(
                "SELECT i.relname, ix.indisunique, a.attname
                 FROM pg_class t
                 JOIN pg_namespace n ON n.oid = t.relnamespace
                 JOIN pg_index ix ON ix.indrelid = t.oid
                 JOIN pg_class i ON i.oid = ix.indexrelid
                 JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS k(attnum, ord) ON true
                 JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = k.attnum
                 WHERE n.nspname = $1 AND t.relname = $2
                 ORDER BY i.relname, k.ord LIMIT 65536",
                &[&schema, &name],
            )
            .map_err(|err| DriverError::Native(format!("读取索引失败：{err}")))?;
        Ok(group_indexes(
            rows.into_iter()
                .map(|row| {
                    (
                        row.get::<_, String>(0),
                        row.get::<_, bool>(1),
                        row.get::<_, String>(2),
                    )
                })
                .collect(),
        ))
    }

    /// pg_constraint 的 conkey/confkey 位置对齐，复合外键不会串列。
    fn foreign_keys(&mut self, table: &str) -> Result<Vec<ForeignKeyEntry>, DriverError> {
        let (schema, name) = split_table(table);
        let rows = self
            .client
            .query(
                "SELECT con.conname, att.attname, fcl.relname, fatt.attname
                 FROM pg_constraint con
                 JOIN pg_class cl ON cl.oid = con.conrelid
                 JOIN pg_namespace ns ON ns.oid = cl.relnamespace
                 JOIN LATERAL unnest(con.conkey, con.confkey) WITH ORDINALITY
                      AS k(attnum, fattnum, ord) ON true
                 JOIN pg_attribute att ON att.attrelid = con.conrelid AND att.attnum = k.attnum
                 JOIN pg_attribute fatt ON fatt.attrelid = con.confrelid AND fatt.attnum = k.fattnum
                 JOIN pg_class fcl ON fcl.oid = con.confrelid
                 WHERE con.contype = 'f' AND ns.nspname = $1 AND cl.relname = $2
                 ORDER BY con.conname, k.ord LIMIT 65536",
                &[&schema, &name],
            )
            .map_err(|err| DriverError::Native(format!("读取外键失败：{err}")))?;
        Ok(group_foreign_keys(
            rows.into_iter()
                .map(|row| {
                    (
                        row.get::<_, String>(0),
                        row.get::<_, String>(1),
                        row.get::<_, String>(2),
                        row.get::<_, String>(3),
                    )
                })
                .collect(),
        ))
    }

    /// 列定义；主键通过 table_constraints 单独查询后合并。
    pub fn columns(&mut self, table: &str) -> Result<Vec<ColumnEntry>, DriverError> {
        let (schema, name) = split_table(table);
        let rows = self
            .client
            .query(
                "SELECT column_name, data_type, is_nullable, column_default,
                    CASE WHEN to_jsonb(c)->>'is_generated' = 'ALWAYS' OR to_jsonb(c)->>'identity_generation' = 'ALWAYS' THEN 'generated' ELSE '' END
                 FROM information_schema.columns c
                 WHERE table_schema = $1 AND table_name = $2
                 ORDER BY ordinal_position LIMIT 4096",
                &[&schema, &name],
            )
            .map_err(|err| DriverError::Native(format!("读取列失败：{err}")))?;

        let primary: Vec<String> = self
            .client
            .query(
                "SELECT kcu.column_name
                 FROM information_schema.table_constraints tc
                 JOIN information_schema.key_column_usage kcu
                   ON tc.constraint_name = kcu.constraint_name
                  AND tc.table_schema = kcu.table_schema
                 WHERE tc.constraint_type = 'PRIMARY KEY'
                   AND tc.table_schema = $1 AND tc.table_name = $2 LIMIT 4096",
                &[&schema, &name],
            )
            .map_err(|err| DriverError::Native(format!("读取主键失败：{err}")))?
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect();

        let comments = self.column_comments(&schema, &name)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let column: String = row.get(0);
                let data_type: String = row.get(1);
                let nullable: String = row.get(2);
                let default_value: Option<String> = row.get(3);
                let comment = comments
                    .iter()
                    .find(|(name, _)| name == &column)
                    .and_then(|(_, comment)| comment.clone());
                ColumnEntry {
                    primary_key: primary.iter().any(|name| name == &column),
                    name: column,
                    data_type,
                    nullable: nullable.eq_ignore_ascii_case("YES"),
                    default_value,
                    extra: row.get(4),
                    comment,
                }
            })
            .collect())
    }
}

/// libpq 风格的 sslMode 归一化。prefer/allow 走明文：不做「先 TLS 再降级」的
/// 二次连接，行为与 libpq 在服务端允许明文时一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SslMode {
    Plain,
    Require,
    Verify,
}

fn ssl_mode(config: &ConnectionConfig) -> Result<SslMode, DriverError> {
    let raw = config.ssl_mode.trim().to_ascii_lowercase();
    let raw = if raw.is_empty() {
        if config.use_ssl {
            "require".to_string()
        } else {
            "disable".to_string()
        }
    } else {
        raw
    };
    // 兼容旧版 Go 配置里的 required / disabled / preferred 等写法。
    match raw.as_str() {
        "disable" | "disabled" | "allow" | "prefer" | "preferred" | "false" | "off" | "none" => {
            Ok(SslMode::Plain)
        }
        "require" | "required" | "true" | "on" | "mandatory" | "strict" | "skip-verify"
        | "insecure" => Ok(SslMode::Require),
        "verify-ca" | "verify-full" | "verify_ca" | "verify_full" => Ok(SslMode::Verify),
        other => Err(DriverError::Native(format!("未知的 sslMode：{other}"))),
    }
}

fn tls_config(config: &ConnectionConfig) -> Result<Option<ClientConfig>, DriverError> {
    match ssl_mode(config)? {
        SslMode::Plain => Ok(None),
        // require 只加密不校验证书（与 libpq 一致），因此用接受任意证书的校验器。
        SslMode::Require => Ok(Some(build_tls(config, false)?)),
        // verify-ca 也做完整校验：rustls 无法只校验链而跳过主机名，宁可更严。
        SslMode::Verify => Ok(Some(build_tls(config, true)?)),
    }
}

fn build_tls(config: &ConnectionConfig, verify: bool) -> Result<ClientConfig, DriverError> {
    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|err| DriverError::Native(format!("TLS 初始化失败：{err}")))?;

    let builder = if verify {
        let mut roots = RootCertStore::empty();
        if !config.ssl_ca_path.trim().is_empty() {
            let pem = std::fs::read(config.ssl_ca_path.trim())
                .map_err(|err| DriverError::Native(format!("读取 CA 失败：{err}")))?;
            let certs = rustls_pemfile::certs(&mut pem.as_slice())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| DriverError::Native(format!("解析 CA 失败：{err}")))?;
            if certs.is_empty() {
                return Err(DriverError::Native("CA 文件没有证书".into()));
            }
            for cert in certs {
                roots
                    .add(cert)
                    .map_err(|err| DriverError::Native(format!("加载 CA 失败：{err}")))?;
            }
        } else {
            for cert in rustls_native_certs::load_native_certs().certs {
                let _ = roots.add(cert);
            }
        }
        builder.with_root_certificates(roots)
    } else {
        builder
            .dangerous()
            .with_custom_certificate_verifier(std::sync::Arc::new(AcceptAnyCerts(provider)))
    };

    let has_cert = !config.ssl_cert_path.trim().is_empty();
    let has_key = !config.ssl_key_path.trim().is_empty();
    if has_cert != has_key {
        return Err(DriverError::Native("客户端证书与私钥必须同时配置".into()));
    }
    let builder = match (has_cert, has_key) {
        (true, true) => {
            let cert_pem = std::fs::read(config.ssl_cert_path.trim())
                .map_err(|err| DriverError::Native(format!("读取客户端证书失败：{err}")))?;
            let key_pem = std::fs::read(config.ssl_key_path.trim())
                .map_err(|err| DriverError::Native(format!("读取客户端私钥失败：{err}")))?;
            let certs = rustls_pemfile::certs(&mut cert_pem.as_slice())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| DriverError::Native(format!("解析客户端证书失败：{err}")))?;
            let key = rustls_pemfile::private_key(&mut key_pem.as_slice())
                .map_err(|err| DriverError::Native(format!("解析客户端私钥失败：{err}")))?
                .ok_or_else(|| DriverError::Native("客户端私钥为空".into()))?;
            builder
                .with_client_auth_cert(certs, key)
                .map_err(|err| DriverError::Native(format!("客户端证书无效：{err}")))?
        }
        _ => builder.with_no_client_auth(),
    };
    Ok(builder)
}

/// sslmode=require 的语义：加密但不对证书做身份校验。
#[derive(Debug)]
struct AcceptAnyCerts(std::sync::Arc<rustls::crypto::CryptoProvider>);

impl ServerCertVerifier for AcceptAnyCerts {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn wrong(err: postgres::Error) -> DriverError {
    DriverError::Native(format!("类型转换失败：{err}"))
}

mod arguments;
use arguments::to_params;

fn to_json(row: &Row, index: usize) -> Result<Value, DriverError> {
    let kind = row.columns()[index].type_().clone();
    match kind {
        Type::BOOL => Ok(json!(row.try_get::<_, Option<bool>>(index).map_err(wrong)?)),
        Type::INT2 => Ok(json!(row.try_get::<_, Option<i16>>(index).map_err(wrong)?)),
        Type::INT4 => Ok(json!(row.try_get::<_, Option<i32>>(index).map_err(wrong)?)),
        Type::INT8 => Ok(json!(row.try_get::<_, Option<i64>>(index).map_err(wrong)?)),
        Type::FLOAT4 => Ok(json!(row.try_get::<_, Option<f32>>(index).map_err(wrong)?)),
        Type::FLOAT8 => Ok(json!(row.try_get::<_, Option<f64>>(index).map_err(wrong)?)),
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::CHAR => Ok(json!(
            row.try_get::<_, Option<String>>(index).map_err(wrong)?
        )),
        Type::BYTEA => Ok(json!(
            row.try_get::<_, Option<Vec<u8>>>(index)
                .map_err(wrong)?
                .map(|bytes| BASE64.encode(bytes))
        )),
        Type::JSON | Type::JSONB => Ok(row
            .try_get::<_, Option<Value>>(index)
            .map_err(wrong)?
            .unwrap_or(Value::Null)),
        Type::DATE => Ok(json!(
            row.try_get::<_, Option<chrono::NaiveDate>>(index)
                .map_err(wrong)?
                .map(|value| value.format("%Y-%m-%d").to_string())
        )),
        Type::TIMESTAMP => Ok(json!(
            row.try_get::<_, Option<chrono::NaiveDateTime>>(index)
                .map_err(wrong)?
                .map(|value| value.format("%Y-%m-%dT%H:%M:%S%.f").to_string())
        )),
        Type::TIMESTAMPTZ => Ok(json!(
            row.try_get::<_, Option<chrono::DateTime<chrono::Utc>>>(index)
                .map_err(wrong)?
                .map(|value| value.to_rfc3339())
        )),
        Type::TIME => Ok(json!(
            row.try_get::<_, Option<chrono::NaiveTime>>(index)
                .map_err(wrong)?
                .map(|value| value.format("%H:%M:%S%.f").to_string())
        )),
        Type::NUMERIC => Ok(json!(
            row.try_get::<_, Option<Numeric>>(index)
                .map_err(wrong)?
                .map(|value| value.0)
        )),
        Type::UUID => Ok(json!(
            row.try_get::<_, Option<PgUuid>>(index)
                .map_err(wrong)?
                .map(|value| value.0)
        )),
        _ => Err(DriverError::Native(format!(
            "暂不支持的 PostgreSQL 类型：{}",
            kind.name()
        ))),
    }
}

/// PostgreSQL NUMERIC 的二进制解码：base-10000 数字 + 标度。
///
/// 布局：int16 ndigits, int16 weight, uint16 sign, uint16 dscale, ndigits × int16。
pub struct Numeric(pub String);

impl FromSql<'_> for Numeric {
    fn from_sql(ty: &Type, raw: &[u8]) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if *ty != Type::NUMERIC {
            return Err(Box::new(WrongType::new::<Numeric>(ty.clone())));
        }
        Ok(Numeric(decode_numeric(raw)?))
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::NUMERIC
    }
}

pub fn decode_numeric(raw: &[u8]) -> Result<String, String> {
    if raw.len() < 8 {
        return Err("numeric 帧过短".into());
    }
    let ndigits = i16::from_be_bytes([raw[0], raw[1]]) as usize;
    let weight = i16::from_be_bytes([raw[2], raw[3]]);
    let sign = u16::from_be_bytes([raw[4], raw[5]]);
    let dscale = u16::from_be_bytes([raw[6], raw[7]]) as usize;
    match sign {
        0xC000 => return Ok("NaN".into()),
        0xD000 => return Ok("Infinity".into()),
        0xF000 => return Ok("-Infinity".into()),
        _ => {}
    }
    if raw.len() < 8 + ndigits * 2 {
        return Err("numeric 位数与帧长不符".into());
    }
    let digits: Vec<i16> = (0..ndigits)
        .map(|index| i16::from_be_bytes([raw[8 + index * 2], raw[9 + index * 2]]))
        .collect();

    let mut integer = String::new();
    if weight >= 0 {
        for position in 0..=weight as usize {
            let digit = digits.get(position).copied().unwrap_or(0);
            if position == 0 {
                integer.push_str(&digit.to_string());
            } else {
                integer.push_str(&format!("{digit:04}"));
            }
        }
    } else {
        integer.push('0');
    }
    while integer.len() > 1 && integer.starts_with('0') {
        integer.remove(0);
    }

    let mut fraction = String::new();
    if dscale > 0 {
        let mut position = weight + 1;
        while fraction.len() < dscale {
            let digit = if position >= 0 {
                digits.get(position as usize).copied().unwrap_or(0)
            } else {
                0
            };
            fraction.push_str(&format!("{digit:04}"));
            position += 1;
        }
        fraction.truncate(dscale);
    }

    let negative = sign == 0x4000;
    let mut output = String::new();
    if negative && !(integer == "0" && fraction.chars().all(|ch| ch == '0')) {
        output.push('-');
    }
    output.push_str(&integer);
    if !fraction.is_empty() {
        output.push('.');
        output.push_str(&fraction);
    }
    Ok(output)
}

/// UUID 的二进制形态是 16 字节，直接转成带连字符的十六进制。
pub struct PgUuid(pub String);

impl FromSql<'_> for PgUuid {
    fn from_sql(ty: &Type, raw: &[u8]) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if *ty != Type::UUID {
            return Err(Box::new(WrongType::new::<PgUuid>(ty.clone())));
        }
        if raw.len() != 16 {
            return Err("uuid 长度不是 16 字节".into());
        }
        let hex: String = raw.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(PgUuid(format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )))
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::UUID
    }
}

#[cfg(test)]
mod tests;
