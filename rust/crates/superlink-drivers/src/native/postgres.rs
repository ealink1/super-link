//! 进程内 PostgreSQL 驱动。
//!
//! 覆盖 Rust 端当前用到的 query 子集：类型化取值、行预算、耗时统计。
//! TLS 连接（sslmode=require 及以上）暂不支持，调用方应回退到 agent。

use std::time::{Duration, Instant};

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
    pub fn open(config: &ConnectionConfig) -> Result<Self, DriverError> {
        let host = if config.host.trim().is_empty() {
            "127.0.0.1"
        } else {
            config.host.trim()
        };
        let mut params = Config::new();
        params
            .host(host)
            .port(if config.port == 0 { 5432 } else { config.port })
            .application_name("SuperLink");
        if !config.user.trim().is_empty() {
            params.user(config.user.trim());
        }
        if !config.password.is_empty() {
            params.password(&config.password);
        }
        if !config.database.trim().is_empty() {
            params.dbname(config.database.trim());
        }
        params.connect_timeout(Duration::from_secs(10));
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
            // 或服务端要求非标准 SSL 协商——两者都需要 Go agent 才能连上。
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

/// 结构浏览：系统 schema 之外的普通表与视图。
impl NativePostgres {
    pub fn tables(&mut self) -> Result<Vec<TableEntry>, DriverError> {
        let rows = self
            .client
            .query(
                "SELECT table_schema, table_name, table_type
                 FROM information_schema.tables
                 WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
                 ORDER BY table_schema, table_name",
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
                 WHERE n.nspname = $1 AND c.relname = $2",
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
                 ORDER BY i.relname, k.ord",
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
                 ORDER BY con.conname, k.ord",
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
                "SELECT column_name, data_type, is_nullable, column_default
                 FROM information_schema.columns
                 WHERE table_schema = $1 AND table_name = $2
                 ORDER BY ordinal_position",
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
                   AND tc.table_schema = $1 AND tc.table_name = $2",
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
    match raw.as_str() {
        "disable" | "allow" | "prefer" | "false" => Ok(SslMode::Plain),
        "require" | "true" => Ok(SslMode::Require),
        "verify-ca" | "verify-full" => Ok(SslMode::Verify),
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

/// 标量参数；复杂类型留给 agent。
fn to_params(args: &[Value]) -> Result<Vec<Box<dyn postgres::types::ToSql + Sync>>, DriverError> {
    args.iter()
        .map(
            |value| -> Result<Box<dyn postgres::types::ToSql + Sync>, DriverError> {
                Ok(match value {
                    Value::Null => Box::new(Option::<String>::None),
                    Value::Bool(flag) => Box::new(*flag),
                    Value::Number(number) => {
                        if let Some(int) = number.as_i64() {
                            Box::new(int)
                        } else {
                            Box::new(number.as_f64().unwrap_or_default())
                        }
                    }
                    Value::String(text) => Box::new(text.clone()),
                    _ => return Err(DriverError::Native("参数只支持标量".into())),
                })
            },
        )
        .collect()
}

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

fn decode_numeric(raw: &[u8]) -> Result<String, String> {
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
mod tests {
    use super::*;

    fn config(ssl_mode: &str, use_ssl: bool) -> ConnectionConfig {
        ConnectionConfig {
            kind: "postgresql".into(),
            ssl_mode: ssl_mode.into(),
            use_ssl,
            ..Default::default()
        }
    }

    #[test]
    fn qualifies_non_public_schemas() {
        assert_eq!(qualify("public", "users"), "users");
        assert_eq!(qualify("app", "users"), "app.users");
        assert_eq!(table_kind("BASE TABLE"), "table");
        assert_eq!(table_kind("VIEW"), "view");
    }

    #[test]
    fn groups_composite_indexes_in_column_order() {
        let grouped = group_indexes(vec![
            ("idx_a".into(), false, "a".into()),
            ("idx_a".into(), false, "b".into()),
            ("idx_pk".into(), true, "id".into()),
        ]);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].columns, vec!["a".to_string(), "b".to_string()]);
        assert!(!grouped[0].unique);
        assert!(grouped[1].unique);
    }

    #[test]
    fn groups_composite_foreign_keys_by_position() {
        let grouped = group_foreign_keys(vec![
            ("fk_pair".into(), "a".into(), "other".into(), "x".into()),
            ("fk_pair".into(), "b".into(), "other".into(), "y".into()),
        ]);
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].columns, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(
            grouped[0].referenced_columns,
            vec!["x".to_string(), "y".to_string()]
        );
        assert_eq!(grouped[0].referenced_table, "other");
    }

    #[test]
    fn splits_schema_qualified_names() {
        assert_eq!(split_table("users"), ("public".into(), "users".into()));
        assert_eq!(split_table("app.users"), ("app".into(), "users".into()));
    }

    #[test]
    fn maps_sslmode_to_a_plan() {
        assert_eq!(ssl_mode(&config("disable", false)).unwrap(), SslMode::Plain);
        assert_eq!(ssl_mode(&config("prefer", false)).unwrap(), SslMode::Plain);
        assert_eq!(ssl_mode(&config("allow", false)).unwrap(), SslMode::Plain);
        assert_eq!(
            ssl_mode(&config("require", false)).unwrap(),
            SslMode::Require
        );
        assert_eq!(
            ssl_mode(&config("verify-full", false)).unwrap(),
            SslMode::Verify
        );
        assert_eq!(
            ssl_mode(&config("verify-ca", false)).unwrap(),
            SslMode::Verify
        );
        // 空值按 useSSL 推断
        assert_eq!(ssl_mode(&config("", true)).unwrap(), SslMode::Require);
        assert_eq!(ssl_mode(&config("", false)).unwrap(), SslMode::Plain);
        assert!(ssl_mode(&config("bogus", false)).is_err());
    }

    #[test]
    fn builds_tls_configs_for_both_verification_modes() {
        let require = tls_config(&config("require", false)).unwrap();
        assert!(require.is_some());
        let verify = tls_config(&config("verify-full", false)).unwrap();
        assert!(verify.is_some());
        assert!(tls_config(&config("disable", false)).unwrap().is_none());
    }

    #[test]
    fn requires_cert_and_key_together() {
        let mut half = config("require", false);
        half.ssl_cert_path = "/tmp/client.pem".into();
        let error = tls_config(&half).unwrap_err().to_string();
        assert!(error.contains("同时配置"), "{error}");
    }

    #[test]
    fn rejects_missing_ca_files() {
        let mut bad = config("verify-full", false);
        bad.ssl_ca_path = "/no/such/ca.pem".into();
        assert!(tls_config(&bad).is_err());
    }

    #[test]
    fn decodes_numeric_values() {
        // 123.45 → ndigits=2, weight=0, sign=0, dscale=2, digits=[123, 4500]
        let raw = [0, 2, 0, 0, 0, 0, 0, 2, 0, 123, 0x11, 0x94];
        assert_eq!(decode_numeric(&raw).unwrap(), "123.45");
        // -0.5 → ndigits=1, weight=-1, sign=0x4000, dscale=1, digits=[5000]
        let raw = [0, 1, 0xFF, 0xFF, 0x40, 0x00, 0, 1, 0x13, 0x88];
        assert_eq!(decode_numeric(&raw).unwrap(), "-0.5");
        // 42 → ndigits=1, weight=0, dscale=0, digits=[42]
        let raw = [0, 1, 0, 0, 0, 0, 0, 0, 0, 42];
        assert_eq!(decode_numeric(&raw).unwrap(), "42");
        // NaN
        let raw = [0, 0, 0, 0, 0xC0, 0x00, 0, 0];
        assert_eq!(decode_numeric(&raw).unwrap(), "NaN");
    }

    /// 真实实例的连接参数；SUPERLINK_TEST_PG=1 开启，
    /// SUPERLINK_TEST_PG_KIND=kingbase 切换方言（默认端口随之变为 54321）。
    fn live_config() -> Option<ConnectionConfig> {
        // SUPERLINK_TEST_PG=1 开启；PG_KIND=kingbase/vastbase 可切换方言。
        std::env::var("SUPERLINK_TEST_PG").ok()?;
        let kind = std::env::var("SUPERLINK_TEST_PG_KIND").unwrap_or_else(|_| "postgres".into());
        let default_port = if kind.eq_ignore_ascii_case("kingbase") {
            54321
        } else {
            5432
        };
        Some(ConnectionConfig {
            kind,
            host: std::env::var("SUPERLINK_TEST_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("SUPERLINK_TEST_PG_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(default_port),
            user: std::env::var("SUPERLINK_TEST_PG_USER").unwrap_or_else(|_| "postgres".into()),
            password: std::env::var("SUPERLINK_TEST_PG_PASSWORD").unwrap_or_default(),
            database: std::env::var("SUPERLINK_TEST_PG_DB").unwrap_or_else(|_| "postgres".into()),
            ssl_mode: std::env::var("SUPERLINK_TEST_PG_SSLMODE").unwrap_or_default(),
            ..Default::default()
        })
    }

    /// 真实实例冒烟：类型解码 + DDL/DML + 行预算 + 错误路径。
    fn live_battery(driver: &mut NativePostgres) {
        let values = driver
            .query(
                "SELECT 1::int4 AS i, 2.5::numeric AS n, 'x'::text AS t, true AS b",
                &[],
                10,
            )
            .unwrap();
        assert_eq!(values.columns.len(), 4);
        assert_eq!(values.rows[0][0], json!(1));
        assert_eq!(values.rows[0][1], json!("2.5"));
        assert_eq!(values.rows[0][2], json!("x"));
        assert_eq!(values.rows[0][3], json!(true));

        driver
            .query(
                "CREATE TEMP TABLE sl_probe(a int, b numeric(10,2), c varchar(20))",
                &[],
                0,
            )
            .unwrap();
        let inserted = driver
            .query(
                "INSERT INTO sl_probe VALUES (1, 12.34, '甲'), (2, 0.50, '乙')",
                &[],
                0,
            )
            .unwrap();
        assert_eq!(inserted.rows_affected, 2, "DDL/DML 的 rows_affected");

        let rows = driver
            .query("SELECT a, b, c FROM sl_probe ORDER BY a", &[], 10)
            .unwrap();
        assert_eq!(rows.rows.len(), 2);
        assert_eq!(rows.rows[0], vec![json!(1), json!("12.34"), json!("甲")]);
        assert_eq!(rows.rows[1][1], json!("0.50"), "numeric 需保留标度");

        let limited = driver
            .query(
                "SELECT a FROM sl_probe UNION ALL SELECT a FROM sl_probe UNION ALL SELECT a FROM sl_probe",
                &[],
                2,
            )
            .unwrap();
        assert_eq!(limited.rows.len(), 2);
        assert!(limited.truncated, "行预算需触发截断");

        let error = driver
            .query("SELECT * FROM missing_table", &[], 0)
            .unwrap_err();
        assert!(!error.to_string().is_empty(), "错误需带服务端信息");
    }

    #[test]
    fn passes_the_battery_on_a_live_pg_server() {
        let Some(config) = live_config() else {
            eprintln!("skipping: SUPERLINK_TEST_PG not set");
            return;
        };
        let mut driver = NativePostgres::open(&config).unwrap();
        live_battery(&mut driver);
    }

    #[test]
    fn formats_uuid_bytes() {
        let raw = [
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88,
        ];
        let value = PgUuid::from_sql(&Type::UUID, &raw).unwrap();
        assert_eq!(value.0, "12345678-9abc-def0-1122-334455667788");
    }
}
