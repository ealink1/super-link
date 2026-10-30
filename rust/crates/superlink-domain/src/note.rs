use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{DomainError, DomainResult};

pub const MAX_NOTE_BODY: usize = 256 << 10;
pub const MAX_NOTEBOOK_BYTES: usize = 6 << 20;
pub const MAX_NOTES: usize = 300;
pub const MAX_NOTE_GROUPS: usize = 50;

/// A local Markdown document; deleted notes stay recoverable in the trash.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Note {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(rename = "groupId", default)]
    pub group_id: String,
    #[serde(default)]
    pub tags: String,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deleted: bool,
}

/// An explicitly named collection; the default group has no id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NoteGroup {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
}

/// The whole notebook is one encrypted atomic snapshot.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Notebook {
    // Go 把空切片写成 null，旧版快照需要按默认值读取。
    #[serde(default, deserialize_with = "null_as_default")]
    pub notes: Vec<Note>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub groups: Vec<NoteGroup>,
    #[serde(skip)]
    pub revision: i64,
}

fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

impl Notebook {
    /// Checks references and budgets without disclosing document contents.
    pub fn validate(&self) -> DomainResult<()> {
        if self.notes.len() > MAX_NOTES || self.groups.len() > MAX_NOTE_GROUPS {
            return Err(DomainError::validation("笔记或分组数量超过限制"));
        }
        let mut group_ids: HashSet<&str> = HashSet::from([""]);
        let mut group_names: HashSet<String> =
            HashSet::from(["默认分组".to_string(), "全部笔记".to_string()]);
        for group in &self.groups {
            let name = group.name.trim().to_lowercase();
            if group.id.is_empty()
                || group_ids.contains(group.id.as_str())
                || name.is_empty()
                || group_names.contains(&name)
                || group.name.chars().count() > 80
            {
                return Err(DomainError::validation("笔记分组无效或重名"));
            }
            group_ids.insert(&group.id);
            group_names.insert(name);
        }
        let mut ids: HashSet<&str> = HashSet::new();
        for note in &self.notes {
            if note.id.is_empty()
                || ids.contains(note.id.as_str())
                || !group_ids.contains(note.group_id.as_str())
            {
                return Err(DomainError::validation("笔记标识或分组无效"));
            }
            if note.body.len() > MAX_NOTE_BODY
                || note.title.chars().count() > 200
                || note.tags.len() > 512
            {
                return Err(DomainError::validation(
                    "笔记格式无效，标题最多 200 字，正文最多 256 KiB",
                ));
            }
            ids.insert(&note.id);
        }
        let raw = serde_json::to_vec(self)
            .map_err(|err| DomainError::validation(format!("笔记本无法序列化：{err}")))?;
        if raw.len() > MAX_NOTEBOOK_BYTES {
            return Err(DomainError::validation("笔记本超过 6 MiB，请先导出并整理"));
        }
        Ok(())
    }

    /// Owns slice storage; immutable strings can be shared between snapshots.
    pub fn clone_deep(&self) -> Self {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, group: &str) -> Note {
        Note {
            id: id.into(),
            group_id: group.into(),
            ..Default::default()
        }
    }

    #[test]
    fn reads_go_style_null_arrays() {
        let book: Notebook = serde_json::from_str(r#"{"notes":null,"groups":null}"#).unwrap();
        assert!(book.notes.is_empty());
        assert!(book.groups.is_empty());

        let empty: Notebook = serde_json::from_str("{}").unwrap();
        assert!(empty.notes.is_empty() && empty.groups.is_empty());
    }

    #[test]
    fn rejects_duplicate_and_reserved_groups() {
        let mut book = Notebook::default();
        book.groups.push(NoteGroup {
            id: "g1".into(),
            name: "默认分组".into(),
        });
        assert!(book.validate().is_err());

        let mut book = Notebook::default();
        book.groups.push(NoteGroup {
            id: "g1".into(),
            name: "Work".into(),
        });
        book.groups.push(NoteGroup {
            id: "g1".into(),
            name: "Home".into(),
        });
        assert!(book.validate().is_err());
    }

    #[test]
    fn rejects_orphan_and_duplicate_notes() {
        let mut book = Notebook::default();
        book.notes.push(note("n1", "missing"));
        assert!(book.validate().is_err());

        let mut book = Notebook::default();
        book.notes.push(note("n1", ""));
        book.notes.push(note("n1", ""));
        assert!(book.validate().is_err());
    }

    #[test]
    fn accepts_a_default_group_note() {
        let mut book = Notebook::default();
        book.notes.push(note("n1", ""));
        assert!(book.validate().is_ok());
    }

    #[test]
    fn enforces_the_title_budget_by_characters() {
        let mut book = Notebook::default();
        book.notes.push(Note {
            id: "n1".into(),
            title: "中".repeat(201),
            ..Default::default()
        });
        assert!(book.validate().is_err());
        book.notes[0].title = "中".repeat(200);
        assert!(book.validate().is_ok());
    }
}
