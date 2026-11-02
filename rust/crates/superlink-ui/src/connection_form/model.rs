//! 表单字段到连接的纯逻辑，便于脱离界面测试。

use superlink_app::Credentials;
use superlink_domain::Profile;

/// 表单里的原始输入，与界面解耦后便于测试。
pub(super) struct FormValues {
    pub(super) kind: String,
    pub(super) name: String,
    pub(super) host: String,
    pub(super) port: String,
    pub(super) user: String,
    pub(super) database: String,
    pub(super) password: String,
    pub(super) ssl_ca: String,
    pub(super) ssl_cert: String,
    pub(super) ssl_key: String,
    pub(super) timeout: String,
    pub(super) query_timeout: String,
    pub(super) use_ssl: bool,
    pub(super) ssl_mode: String,
}

fn parse_seconds(raw: &str) -> Result<u32, String> {
    if raw.trim().is_empty() {
        return Ok(0);
    }
    match raw.trim().parse::<u32>() {
        Ok(value) if value <= 3600 => Ok(value),
        _ => Err("超时必须是 0–3600 的整数（秒）".into()),
    }
}

/// 校验并组装待保存的连接；编辑时保留 id、revision 与表单未管理的字段。
pub(super) fn compose(
    values: &FormValues,
    editing: Option<&Profile>,
) -> Result<(Profile, Credentials), String> {
    let name = values.name.trim();
    if name.is_empty() {
        return Err("请输入连接名称".into());
    }
    let port: u16 = if values.port.trim().is_empty() {
        0
    } else {
        values
            .port
            .trim()
            .parse()
            .map_err(|_| "端口无效，请输入 1-65535".to_string())?
    };
    if values.kind == "sqlite" {
        if values.database.trim().is_empty() {
            return Err("请输入 SQLite 数据库文件路径".into());
        }
    } else if values.kind != "custom" {
        if values.host.trim().is_empty() {
            return Err("请输入主机地址".into());
        }
        if port == 0 {
            return Err(if values.port.trim().is_empty() {
                "请输入端口（1-65535）"
            } else {
                "端口无效，请输入 1-65535"
            }
            .to_string());
        }
    }
    let timeout = parse_seconds(&values.timeout)?;
    let query_timeout = parse_seconds(&values.query_timeout)?;

    let mut config = editing
        .map(|profile| profile.config.clone())
        .unwrap_or_default();
    config.kind = values.kind.clone();
    config.host = values.host.trim().to_string();
    config.port = port;
    config.user = values.user.trim().to_string();
    config.database = values.database.trim().to_string();
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

    let mut profile = editing.cloned().unwrap_or_default();
    profile.name = name.to_string();
    profile.config = config;
    if profile.id.is_empty() {
        profile.id = uuid::Uuid::new_v4().to_string();
    }
    let credentials = Credentials {
        password: values.password.trim().to_string(),
        ..Default::default()
    };
    Ok((profile, credentials))
}

#[cfg(test)]
mod tests {
    use super::*;
    use superlink_domain::ConnectionConfig;

    fn values(kind: &str) -> FormValues {
        FormValues {
            kind: kind.into(),
            name: "生产库".into(),
            host: "10.0.0.5".into(),
            port: "3306".into(),
            user: "root".into(),
            database: "app".into(),
            password: "secret".into(),
            ssl_ca: String::new(),
            ssl_cert: String::new(),
            ssl_key: String::new(),
            timeout: String::new(),
            query_timeout: String::new(),
            use_ssl: false,
            ssl_mode: "required".into(),
        }
    }

    #[test]
    fn builds_a_new_profile_with_ssl_and_timeouts() {
        let mut input = values("mysql");
        input.use_ssl = true;
        input.ssl_mode = "verify-ca".into();
        input.ssl_ca = " /etc/ca.pem ".into();
        input.timeout = "15".into();
        input.query_timeout = "60".into();

        let (profile, credentials) = compose(&input, None).unwrap();
        assert!(!profile.id.is_empty());
        assert_eq!(profile.name, "生产库");
        assert_eq!(profile.config.kind, "mysql");
        assert_eq!(profile.config.host, "10.0.0.5");
        assert_eq!(profile.config.port, 3306);
        assert!(profile.config.use_ssl);
        assert_eq!(profile.config.ssl_mode, "verify-ca");
        assert_eq!(profile.config.ssl_ca_path, "/etc/ca.pem");
        assert_eq!(profile.config.timeout, 15);
        assert_eq!(profile.config.query_timeout, 60);
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
                connection_params: "charset=utf8mb4".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut input = values("mysql");
        input.name = "新名称".into();
        input.port = "3307".into();

        let (profile, _) = compose(&input, Some(&editing)).unwrap();
        assert_eq!(profile.id, "p1");
        assert_eq!(profile.revision, 7);
        assert_eq!(profile.secret_ref, "local:v1:abc");
        assert_eq!(profile.group, "ysy");
        assert!(profile.read_only);
        assert_eq!(profile.name, "新名称");
        assert_eq!(profile.config.port, 3307);
        // 表单未管理的字段保持原值
        assert_eq!(profile.config.connection_params, "charset=utf8mb4");
    }

    #[test]
    fn clears_ssl_mode_when_the_toggle_is_off() {
        let input = values("mysql");
        let (profile, _) = compose(&input, None).unwrap();
        assert!(!profile.config.use_ssl);
        assert!(profile.config.ssl_mode.is_empty());
    }

    #[test]
    fn rejects_missing_host_path_and_bad_port() {
        let mut input = values("mysql");
        input.host = "  ".into();
        assert_eq!(compose(&input, None).unwrap_err(), "请输入主机地址");

        let mut input = values("mysql");
        input.port = "0".into();
        assert_eq!(
            compose(&input, None).unwrap_err(),
            "端口无效，请输入 1-65535"
        );

        let mut input = values("mysql");
        input.port.clear();
        assert_eq!(compose(&input, None).unwrap_err(), "请输入端口（1-65535）");

        let mut input = values("sqlite");
        input.database = "   ".into();
        assert_eq!(
            compose(&input, None).unwrap_err(),
            "请输入 SQLite 数据库文件路径"
        );
    }

    #[test]
    fn rejects_timeouts_above_the_limit() {
        let mut input = values("mysql");
        input.timeout = "3601".into();
        assert!(compose(&input, None).is_err());

        let mut input = values("mysql");
        input.query_timeout = "abc".into();
        assert!(compose(&input, None).is_err());
    }
}
