//! 表单字段到连接的纯逻辑，便于脱离界面测试。
//!
//! 校验规则对齐 Fyne 版：端口空为 0、上限 65535；连接超时 0–120，
//! 查询超时 0–3600；连接串非空时由它决定主机、端口与凭据。

use serde_json::{Value, json};

use superlink_app::Credentials;
use superlink_domain::{Profile, split_patterns};

pub(super) const CONNECT_TIMEOUT_MAX: u32 = 120;
pub(super) const QUERY_TIMEOUT_MAX: u32 = 3600;

/// 表单里的原始输入，与界面解耦后便于测试。
pub(super) struct FormValues {
    pub(super) kind: String,
    pub(super) name: String,
    pub(super) group: String,
    pub(super) environment: String,
    pub(super) host: String,
    pub(super) port: String,
    pub(super) user: String,
    pub(super) database: String,
    pub(super) password: String,
    pub(super) uri: String,
    pub(super) dsn: String,
    pub(super) driver: String,
    pub(super) connection_params: String,
    pub(super) hosts: String,
    pub(super) topology: String,
    pub(super) allow: String,
    pub(super) include: String,
    pub(super) exclude: String,
    pub(super) ssl_ca: String,
    pub(super) ssl_cert: String,
    pub(super) ssl_key: String,
    pub(super) timeout: String,
    pub(super) query_timeout: String,
    pub(super) use_ssl: bool,
    pub(super) ssl_mode: String,
    pub(super) read_only: bool,
    pub(super) protect_edit: bool,
    pub(super) protect_structure: bool,
    pub(super) protect_script: bool,
    pub(super) protect_import: bool,
    pub(super) icon_type: String,
    pub(super) icon_color: String,
}

/// 校验并组装待保存的连接；编辑时保留 id、revision 与表单未管理的字段。
pub(super) fn compose(
    values: &FormValues,
    editing: Option<&Profile>,
    use_uri: bool,
) -> Result<(Profile, Credentials), String> {
    let name = values.name.trim();
    if name.is_empty() {
        return Err("请填写连接名称".into());
    }
    let port = parse_port(&values.port)?;
    let uri = values.uri.trim();
    if use_uri && uri.is_empty() {
        return Err("连接串为空，请改用主机 / 端口字段或清空后重新生成".into());
    }
    let local = matches!(values.kind.as_str(), "sqlite" | "duckdb")
        || values.kind == "custom"
            && matches!(values.driver.as_str(), "sqlite" | "sqlite3" | "duckdb");
    if values.kind == "sqlite" || values.kind == "duckdb" {
        if values.database.trim().is_empty() {
            return Err("请填写数据库文件路径，或在主机栏使用 :memory:".into());
        }
    } else if !use_uri && values.kind != "custom" && !local {
        // 端口留空表示使用驱动默认值（与旧版 parsePort 一致），仅拒绝越界输入。
        if values.host.trim().is_empty() {
            return Err("请输入主机地址".into());
        }
    }
    if use_uri && matches!(values.kind.as_str(), "sqlite" | "duckdb" | "custom") {
        return Err("本地数据库请使用文件路径；自定义驱动请使用 DSN 字段".into());
    }
    let timeout = parse_timeout(&values.timeout, CONNECT_TIMEOUT_MAX)?;
    let query_timeout = parse_timeout(&values.query_timeout, QUERY_TIMEOUT_MAX)?;

    let mut config = editing
        .map(|profile| profile.config.clone())
        .unwrap_or_default();
    config.kind = values.kind.clone();
    config.host = values.host.trim().to_string();
    config.port = port;
    config.user = values.user.trim().to_string();
    config.database = values.database.trim().to_string();
    config.uri = uri.to_string();
    config.dsn = values.dsn.trim().to_string();
    config.driver = values.driver.trim().to_string();
    config.connection_params = super::uri::normalize_params(&values.connection_params);
    config.hosts = split_patterns(&values.hosts);
    config.topology = if local {
        String::new()
    } else {
        values.topology.trim().to_string()
    };
    config.use_ssl = values.use_ssl;
    config.ssl_mode = if values.use_ssl {
        values.ssl_mode.clone()
    } else {
        String::new()
    };
    config.ssl_ca_path = values.ssl_ca.trim().to_string();
    config.ssl_cert_path = values.ssl_cert.trim().to_string();
    config.ssl_key_path = values.ssl_key.trim().to_string();
    config.timeout = timeout;
    config.query_timeout = query_timeout;
    config.read_only = values.read_only;
    config.protection = Some(json!({
        "restrictDataEdit": values.protect_edit,
        "restrictStructureEdit": values.protect_structure,
        "restrictScriptExecution": values.protect_script,
        "restrictDataImport": values.protect_import,
    }));

    let mut profile = editing.cloned().unwrap_or_default();
    profile.name = name.to_string();
    profile.group = values.group.trim().to_string();
    profile.icon_type = values.icon_type.trim().to_string();
    profile.icon_color = values.icon_color.trim().to_string();
    profile.database_allow = split_patterns(&values.allow);
    profile.database_include = split_patterns(&values.include);
    profile.database_exclude = split_patterns(&values.exclude);
    profile.read_only = values.read_only;
    let environment = values.environment.trim();
    profile.environment = if environment.is_empty() {
        "dev".to_string()
    } else {
        environment.to_string()
    };
    profile.config = config;
    if profile.id.is_empty() {
        profile.id = uuid::Uuid::new_v4().to_string();
    }
    profile
        .validate_presentation()
        .map_err(|err| err.to_string())?;

    let credentials = Credentials {
        password: values.password.clone(),
        ..Default::default()
    };
    Ok((profile, credentials))
}

fn parse_port(raw: &str) -> Result<u16, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Ok(0);
    }
    match text.parse::<u32>() {
        Ok(value) if value <= 65535 => Ok(value as u16),
        _ => Err("端口应为 0–65535 的整数".into()),
    }
}

fn parse_timeout(raw: &str, max: u32) -> Result<u32, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Ok(0);
    }
    match text.parse::<u32>() {
        Ok(value) if value <= max => Ok(value),
        _ => Err(format!("超时必须是 0–{max} 的整数（秒）")),
    }
}

/// 连接串是否应该优先于各字段：只有非空的连接串才提升为权威。
pub(super) fn uri_takes_over(uri: &str) -> bool {
    !uri.trim().is_empty()
}

/// 把高级配置 JSON 叠到已组装的配置上；空文本视为未填写。
pub(super) fn overlay_extra(profile: &mut Profile, text: &str) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("高级配置 JSON 无效：{e}"))?;
    let Some(patch) = value.as_object() else {
        return Err("高级配置应为 JSON 对象".into());
    };
    let mut base = serde_json::to_value(&profile.config).unwrap_or_default();
    superlink_domain::secrets::merge(base.as_object_mut().unwrap(), patch);
    profile.config = serde_json::from_value(base).map_err(|e| format!("高级配置无效：{e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use superlink_domain::ConnectionConfig;

    pub(super) fn values(kind: &str) -> FormValues {
        FormValues {
            kind: kind.into(),
            name: "生产库".into(),
            group: "ysy".into(),
            environment: "test".into(),
            host: "10.0.0.5".into(),
            port: "3306".into(),
            user: "root".into(),
            database: "app".into(),
            password: "secret".into(),
            uri: String::new(),
            dsn: String::new(),
            driver: String::new(),
            connection_params: "".into(),
            hosts: String::new(),
            topology: "single".into(),
            allow: String::new(),
            include: String::new(),
            exclude: String::new(),
            ssl_ca: String::new(),
            ssl_cert: String::new(),
            ssl_key: String::new(),
            timeout: String::new(),
            query_timeout: String::new(),
            use_ssl: false,
            ssl_mode: "required".into(),
            read_only: false,
            protect_edit: false,
            protect_structure: false,
            protect_script: false,
            protect_import: false,
            icon_type: String::new(),
            icon_color: String::new(),
        }
    }

    #[test]
    fn builds_a_new_profile_with_ssl_timeouts_and_protection() {
        let mut input = values("mysql");
        input.use_ssl = true;
        input.ssl_mode = "verify-ca".into();
        input.ssl_ca = " /etc/ca.pem ".into();
        input.timeout = "15".into();
        input.query_timeout = "60".into();
        input.protect_edit = true;
        input.read_only = true;
        input.hosts = "a:1, b:2".into();
        input.allow = "app, shop".into();

        let (profile, credentials) = compose(&input, None, false).unwrap();
        assert!(!profile.id.is_empty());
        assert_eq!(profile.name, "生产库");
        assert_eq!(profile.group, "ysy");
        assert_eq!(profile.environment, "test");
        assert_eq!(profile.config.kind, "mysql");
        assert_eq!(profile.config.port, 3306);
        assert!(profile.config.use_ssl);
        assert_eq!(profile.config.ssl_mode, "verify-ca");
        assert_eq!(profile.config.ssl_ca_path, "/etc/ca.pem");
        assert_eq!(profile.config.timeout, 15);
        assert_eq!(profile.config.query_timeout, 60);
        assert!(profile.read_only && profile.config.read_only);
        assert_eq!(profile.config.topology, "single");
        assert_eq!(profile.config.hosts, vec!["a:1", "b:2"]);
        assert_eq!(profile.database_allow, vec!["app", "shop"]);
        let protection = profile.config.protection.as_ref().unwrap();
        assert_eq!(protection["restrictDataEdit"], json!(true));
        assert_eq!(protection["restrictDataImport"], json!(false));
        assert_eq!(credentials.password, "secret");
    }

    #[test]
    fn keeps_identity_and_unmanaged_fields_when_editing() {
        let editing = Profile {
            id: "p1".into(),
            name: "旧名称".into(),
            group: "ysy".into(),
            read_only: true,
            revision: 7,
            secret_ref: "local:v1:abc".into(),
            config: ConnectionConfig {
                keep_alive_enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut input = values("mysql");
        input.name = "新名称".into();
        input.port = "3307".into();

        let (profile, _) = compose(&input, Some(&editing), false).unwrap();
        assert_eq!(profile.id, "p1");
        assert_eq!(profile.revision, 7);
        assert_eq!(profile.secret_ref, "local:v1:abc");
        assert_eq!(profile.name, "新名称");
        assert_eq!(profile.config.port, 3307);
        // 表单未管理的字段保持原值
        assert!(profile.config.keep_alive_enabled);
    }

    #[test]
    fn clears_ssl_mode_when_the_toggle_is_off() {
        let input = values("mysql");
        let (profile, _) = compose(&input, None, false).unwrap();
        assert!(!profile.config.use_ssl);
        assert!(profile.config.ssl_mode.is_empty());
    }

    #[test]
    fn accepts_empty_or_zero_ports_like_the_old_form() {
        let mut input = values("mysql");
        input.port.clear();
        let (profile, _) = compose(&input, None, false).unwrap();
        assert_eq!(profile.config.port, 0);

        let mut input = values("mysql");
        input.port = "65536".into();
        assert_eq!(
            compose(&input, None, false).unwrap_err(),
            "端口应为 0–65535 的整数"
        );
    }

    #[test]
    fn rejects_missing_host_path_and_bad_name() {
        let mut input = values("mysql");
        input.host = "  ".into();
        assert_eq!(compose(&input, None, false).unwrap_err(), "请输入主机地址");

        let mut input = values("sqlite");
        input.database = "   ".into();
        assert!(compose(&input, None, false).is_err());

        let mut input = values("mysql");
        input.name = " ".into();
        assert_eq!(compose(&input, None, false).unwrap_err(), "请填写连接名称");
    }

    #[test]
    fn uri_mode_skips_host_checks_and_rejects_local_types() {
        let mut input = values("mysql");
        input.host.clear();
        input.port.clear();
        input.uri = "mysql://u@h/db".into();
        let (profile, _) = compose(&input, None, true).unwrap();
        assert_eq!(profile.config.uri, "mysql://u@h/db");

        let mut input = values("mysql");
        input.uri = "  ".into();
        assert!(compose(&input, None, true).is_err());

        let mut input = values("sqlite");
        input.uri = "sqlite:///tmp/x.db".into();
        assert!(compose(&input, None, true).is_err());
    }

    #[test]
    fn validates_timeout_ranges_like_the_old_form() {
        let mut input = values("mysql");
        input.timeout = "121".into();
        assert!(compose(&input, None, false).is_err());
        input.timeout = "120".into();
        assert!(compose(&input, None, false).is_ok());

        let mut input = values("mysql");
        input.query_timeout = "3601".into();
        assert!(compose(&input, None, false).is_err());
        input.query_timeout = "abc".into();
        assert!(compose(&input, None, false).is_err());
    }

    #[test]
    fn rejects_invalid_icon_color() {
        let mut input = values("mysql");
        input.icon_type = "redis".into();
        input.icon_color = "#47A248".into();
        assert!(compose(&input, None, false).is_ok());
        input.icon_color = "green".into();
        assert!(compose(&input, None, false).is_err());
    }

    #[test]
    fn stores_uri_and_merged_params_for_the_driver() {
        let mut input = values("mysql");
        input.uri = "mysql://u:p@h:3307/app?ssl-mode=required".into();
        input.connection_params = "?charset=utf8mb4".into();
        let (profile, credentials) = compose(&input, None, true).unwrap();
        assert_eq!(
            profile.config.uri,
            "mysql://u:p@h:3307/app?ssl-mode=required"
        );
        assert_eq!(profile.config.connection_params, "charset=utf8mb4");
        assert_eq!(credentials.password, "secret");
        let merged = profile.merged_connection_params();
        assert!(merged.contains("ssl-mode=required"));
        assert!(merged.contains("charset=utf8mb4"));
    }

    #[test]
    fn defaults_environment_and_clears_topology_for_local_files() {
        let mut input = values("mysql");
        input.environment = "  ".into();
        let (profile, _) = compose(&input, None, false).unwrap();
        assert_eq!(profile.environment, "dev");

        let mut input = values("sqlite");
        input.database = "/tmp/qa.db".into();
        input.topology = "cluster".into();
        let (profile, _) = compose(&input, None, false).unwrap();
        assert!(profile.config.topology.is_empty());
        assert_eq!(profile.config.database, "/tmp/qa.db");
    }

    #[test]
    fn overlay_extra_rejects_bad_json_and_objects_only() {
        let (mut profile, _) = compose(&values("mysql"), None, false).unwrap();
        assert!(overlay_extra(&mut profile, "").is_ok());
        assert!(overlay_extra(&mut profile, "{").is_err());
        assert!(overlay_extra(&mut profile, "[1]").is_err());
        assert!(overlay_extra(&mut profile, r#"{"keepAliveEnabled":true}"#).is_ok());
        assert!(profile.config.keep_alive_enabled);
    }
}
