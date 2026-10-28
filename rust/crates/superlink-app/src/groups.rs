//! 连接分组：分组清单存设置，连接归属存 profile.group。

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::json;

use crate::profiles::{ProfileService, ServiceResult};
use crate::settings::SettingsService;

/// 分组清单在设置里的键。
const GROUPS_KEY: &str = "connection.groups";
/// 默认分组名（空归属也归到这里显示）。
pub const DEFAULT_GROUP: &str = "默认";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupSummary {
    pub name: String,
    pub count: usize,
}

/// 合并设置里的分组清单与连接上的归属，空归属显示为「默认」。
pub fn summarize(profiles: &[superlink_domain::Profile], declared: &[String]) -> Vec<GroupSummary> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for profile in profiles {
        names.insert(display_group(&profile.group));
    }
    for name in declared {
        names.insert(display_group(name));
    }
    names
        .into_iter()
        .map(|name| GroupSummary {
            count: profiles
                .iter()
                .filter(|profile| display_group(&profile.group) == name)
                .count(),
            name,
        })
        .collect()
}

/// 展示名：空归属映射到默认分组。
pub fn display_group(group: &str) -> String {
    if group.trim().is_empty() {
        DEFAULT_GROUP.to_string()
    } else {
        group.trim().to_string()
    }
}

/// 把 from 组的连接改到 to 组；返回改动数量。
pub fn reassign(profiles: &mut [superlink_domain::Profile], from: &str, to: &str) -> usize {
    let from = display_group(from);
    let to = if to.trim().is_empty() || to == DEFAULT_GROUP {
        String::new()
    } else {
        to.trim().to_string()
    };
    let mut changed = 0;
    for profile in profiles.iter_mut() {
        if display_group(&profile.group) == from {
            profile.group = to.clone();
            changed += 1;
        }
    }
    changed
}

/// 分组管理服务：清单与连接归属一起维护。
pub struct GroupService {
    profiles: Arc<ProfileService>,
    settings: Arc<SettingsService>,
}

impl GroupService {
    pub fn new(profiles: Arc<ProfileService>, settings: Arc<SettingsService>) -> Self {
        Self { profiles, settings }
    }

    /// 显式声明的分组（不含默认分组）。
    pub fn declared(&self) -> Vec<String> {
        self.settings
            .get(GROUPS_KEY)
            .and_then(|value| value.as_array().cloned())
            .map(|items| {
                items
                    .into_iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn write_declared(&self, groups: &[String]) {
        let value = json!(groups);
        self.settings.set(GROUPS_KEY, value);
    }

    pub fn summaries(&self) -> ServiceResult<Vec<GroupSummary>> {
        let profiles = self.profiles.list()?;
        Ok(summarize(&profiles, &self.declared()))
    }

    /// 新建空分组；重名或空名返回错误。
    pub fn create(&self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("分组名不能为空".into());
        }
        if name == DEFAULT_GROUP {
            return Err("默认分组已存在".into());
        }
        let mut groups = self.declared();
        if groups.iter().any(|group| group == name) {
            return Err("分组已存在".into());
        }
        groups.push(name.to_string());
        groups.sort();
        self.write_declared(&groups);
        Ok(())
    }

    /// 重命名分组：清单与连接归属一起更新。
    pub fn rename(&self, from: &str, to: &str) -> Result<usize, String> {
        let to = to.trim();
        if to.is_empty() {
            return Err("新分组名不能为空".into());
        }
        let from = display_group(from);
        if from == to {
            return Ok(0);
        }
        let mut groups: Vec<String> = self
            .declared()
            .into_iter()
            .map(|group| if group == from { to.to_string() } else { group })
            .collect();
        if !groups.iter().any(|group| group == to) {
            groups.push(to.to_string());
        }
        if groups.iter().filter(|group| *group == to).count() > 1 {
            return Err("目标分组名已存在".into());
        }
        groups.sort();
        groups.dedup();
        let mut profiles = self.profiles.list().map_err(|err| err.to_string())?;
        let changed = reassign(&mut profiles, &from, to);
        for profile in profiles
            .iter()
            .filter(|profile| display_group(&profile.group) == to)
        {
            self.profiles
                .save(profile.clone(), &Default::default())
                .map_err(|err| err.to_string())?;
        }
        self.write_declared(&groups);
        Ok(changed)
    }

    /// 删除分组：连接回到默认分组。
    pub fn remove(&self, name: &str) -> Result<usize, String> {
        let name = display_group(name);
        if name == DEFAULT_GROUP {
            return Err("默认分组不能删除".into());
        }
        let groups: Vec<String> = self
            .declared()
            .into_iter()
            .filter(|group| group != &name)
            .collect();
        self.write_declared(&groups);
        let mut profiles = self.profiles.list().map_err(|err| err.to_string())?;
        let changed = reassign(&mut profiles, &name, "");
        for profile in profiles.iter().filter(|profile| profile.group.is_empty()) {
            self.profiles
                .save(profile.clone(), &Default::default())
                .map_err(|err| err.to_string())?;
        }
        Ok(changed)
    }

    /// 把一个连接移动到目标分组。
    pub fn move_profile(&self, id: &str, to: &str) -> Result<(), String> {
        let mut profile = self.profiles.get(id).map_err(|err| err.to_string())?;
        profile.group = if to.trim().is_empty() || to == DEFAULT_GROUP {
            String::new()
        } else {
            to.trim().to_string()
        };
        self.profiles
            .save(profile, &Default::default())
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use superlink_domain::Profile;
    use superlink_infra::Store;

    fn profile(name: &str, group: &str) -> Profile {
        Profile {
            name: name.into(),
            group: group.into(),
            ..Default::default()
        }
    }

    #[test]
    fn summarizes_declared_and_implicit_groups() {
        let profiles = vec![profile("a", ""), profile("b", "生产"), profile("c", "生产")];
        let summary = summarize(&profiles, &["测试".to_string()]);
        assert!(
            summary
                .iter()
                .any(|group| group.name == DEFAULT_GROUP && group.count == 1)
        );
        assert!(
            summary
                .iter()
                .any(|group| group.name == "生产" && group.count == 2)
        );
        assert!(
            summary
                .iter()
                .any(|group| group.name == "测试" && group.count == 0)
        );
    }

    #[test]
    fn reassign_moves_only_matching_group() {
        let mut profiles = vec![profile("a", "生产"), profile("b", "测试")];
        assert_eq!(reassign(&mut profiles, "生产", DEFAULT_GROUP), 1);
        assert_eq!(profiles[0].group, "");
        assert_eq!(profiles[1].group, "测试");
    }

    #[test]
    fn service_creates_renames_and_removes_groups() {
        let dir = std::env::temp_dir().join(format!("superlink-groups-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Arc::new(Store::open(dir.join("state.db")).unwrap());
        let vault = Arc::new(superlink_infra::LocalVault::new(dir.join("vault.key")));
        let profiles = Arc::new(ProfileService::new(store.clone(), vault));
        let settings = Arc::new(SettingsService::new(store.clone()));
        let service = GroupService::new(profiles.clone(), settings.clone());

        service.create("生产").unwrap();
        assert!(service.create("生产").is_err(), "重复分组名应被拒绝");

        let mut saved = profile("a", "生产");
        saved.config.kind = "sqlite".into();
        saved.config.database = "/tmp/a.db".into();
        profiles.save(saved, &Default::default()).unwrap();

        assert_eq!(service.rename("生产", "线上").unwrap(), 1);
        let list = profiles.list().unwrap();
        assert_eq!(list[0].group, "线上");

        assert_eq!(service.remove("线上").unwrap(), 1);
        let list = profiles.list().unwrap();
        assert!(list[0].group.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
