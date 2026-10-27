use serde::{Deserialize, Serialize};

use crate::error::{DomainError, DomainResult};

/// Public SSH endpoint metadata. Credentials never serialize.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ShellHost {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "group", default)]
    pub group: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub port: u16,
    #[serde(rename = "keyPath", default)]
    pub key_path: String,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub remember: bool,
    #[serde(default)]
    pub revision: i64,
    #[serde(skip)]
    pub secret_ref: String,
    #[serde(skip)]
    pub password: String,
    #[serde(skip)]
    pub passphrase: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub tags: String,
    #[serde(rename = "operatingSystem", default)]
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
