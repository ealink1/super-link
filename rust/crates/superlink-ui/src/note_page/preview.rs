use super::*;
#[path = "markdown_preview.rs"]
mod markdown_preview;
pub(super) use markdown_preview::markdown_blocks;
pub(super) fn trimmed(body: &str) -> String {
    if body.len() > PREVIEW_BYTES {
        format!(
            "{}…",
            String::from_utf8_lossy(&body.as_bytes()[..PREVIEW_BYTES])
        )
    } else {
        body.to_string()
    }
}

pub(super) fn snippet(body: &str, limit: usize) -> String {
    body.lines()
        .map(|line| line.trim().trim_start_matches(['#', '>', '-', '*']).trim())
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(limit).collect())
        .unwrap_or_default()
}

pub(super) fn stamp(note: &Note) -> String {
    note.updated_at
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%m/%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}
