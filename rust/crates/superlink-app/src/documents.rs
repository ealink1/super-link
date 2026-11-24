//! Bounded user-selected text documents; filesystem work belongs on a worker.
use std::path::{Path, PathBuf};
use superlink_domain::Note;
use superlink_files::{cancel::NeverCancelled, query_file};

pub fn read_document(path: &Path) -> Result<String, String> {
    query_file::read(&NeverCancelled, path).map_err(|e| e.to_string())
}
pub fn write_document(path: &Path, text: &str) -> Result<(), String> {
    query_file::save(&NeverCancelled, path, text, true).map_err(|e| e.to_string())
}
pub fn read_notes(paths: &[PathBuf], group: &str) -> Result<Vec<Note>, String> {
    if paths.len() > superlink_domain::note::MAX_NOTES {
        return Err("一次导入的笔记过多".into());
    }
    let mut notes = Vec::new();
    for path in paths {
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if !matches!(
            extension.to_ascii_lowercase().as_str(),
            "md" | "markdown" | "txt"
        ) {
            return Err("请选择 Markdown 或文本文件".into());
        }
        let body = read_document(path)?;
        if body.len() > superlink_domain::note::MAX_NOTE_BODY {
            return Err("单篇笔记不能超过 256 KiB".into());
        }
        notes.push(Note {
            id: uuid::Uuid::new_v4().to_string(),
            title: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .take(200)
                .collect(),
            body,
            group_id: group.into(),
            updated_at: Some(chrono::Utc::now()),
            ..Default::default()
        });
    }
    Ok(notes)
}
