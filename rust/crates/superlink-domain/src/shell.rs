use serde::{Deserialize, Serialize};

use crate::error::{DomainError, DomainResult};

/// Public SSH endpoint metadata. Credentials never serialize.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ShellHost {
    // 旧版 Go 直接序列化结构体字段（PascalCase），用别名继续读取那段数据。
    #[serde(default, alias = "ID")]
    pub id: String,
    #[serde(default, alias = "Name")]
    pub name: String,
    #[serde(rename = "group", default, alias = "Group")]
    pub group: String,
    #[serde(default, alias = "Host")]
    pub host: String,
    #[serde(default, alias = "User")]
    pub user: String,
    #[serde(default, alias = "Port")]
    pub port: u16,
    #[serde(rename = "keyPath", default, alias = "KeyPath")]
    pub key_path: String,
    #[serde(default, alias = "Fingerprint")]
    pub fingerprint: String,
    #[serde(default, alias = "Remember")]
    pub remember: bool,
    #[serde(default, alias = "Revision")]
    pub revision: i64,
    #[serde(skip)]
    pub secret_ref: String,
    #[serde(skip)]
    pub password: String,
    #[serde(skip)]
    pub passphrase: String,
    #[serde(default, alias = "Notes")]
    pub notes: String,
    #[serde(default, alias = "Tags")]
    pub tags: String,
    #[serde(rename = "operatingSystem", default, alias = "OperatingSystem")]
    pub operating_system: String,
}

impl ShellHost {
    /// Validates without ever interpreting host input as a command.
    pub fn validate(&self) -> DomainResult<()> {
        if self.name.trim().is_empty() || self.name.len() > 256 {
            return Err(DomainError::validation("主机名称不能为空，且最多 256 字节"));
        }
        if self.host.trim().is_empty()
            || self.host.contains(['\0', '\r', '\n', '/', ' '])
            || self.host.len() > 253
        {
            return Err(DomainError::validation("请输入有效的 SSH 主机地址"));
        }
        if self.port == 0
            || self.user.trim().is_empty()
            || self.user.len() > 256
            || self.user.contains(['\0', '\r', '\n'])
        {
            return Err(DomainError::validation("SSH 端口或用户名无效"));
        }
        if self.group.len() > 256 || self.fingerprint.len() > 256 || self.key_path.len() > 4096 {
            return Err(DomainError::validation("主机分组或指纹过长"));
        }
        if self.notes.len() > 4096 || self.tags.len() > 512 || self.operating_system.len() > 64 {
            return Err(DomainError::validation("主机备注、标签或系统名称过长"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> ShellHost {
        ShellHost {
            name: "prod".into(),
            host: "10.0.0.1".into(),
            user: "root".into(),
            port: 22,
            ..Default::default()
        }
    }

    #[test]
    fn accepts_a_normal_host() {
        assert!(host().validate().is_ok());
    }

    #[test]
    fn reads_hosts_written_by_the_go_build() {
        let raw = r#"{"ID":"h1","Name":"ysy104","Group":"","Host":"192.168.0.104",
            "User":"kaifa","Port":22,"KeyPath":"","Remember":true,"Revision":2,
            "Notes":"","Tags":"","OperatingSystem":"Linux"}"#;
        let host: ShellHost = serde_json::from_str(raw).unwrap();
        assert_eq!(host.id, "h1");
        assert_eq!(host.name, "ysy104");
        assert_eq!(host.host, "192.168.0.104");
        assert_eq!(host.user, "kaifa");
        assert_eq!(host.port, 22);
        assert!(host.remember);
        assert_eq!(host.revision, 2);
        assert_eq!(host.operating_system, "Linux");
    }

    #[test]
    fn rejects_command_injection_in_the_host_field() {
        for candidate in ["a b", "a/b", "a\nb", "a;rm -rf"] {
            let mut host = host();
            host.host = candidate.into();
            assert!(host.validate().is_err(), "{candidate} must be rejected");
        }
    }

    #[test]
    fn rejects_out_of_range_ports() {
        let mut host = host();
        host.port = 0;
        assert!(host.validate().is_err());
    }
}
