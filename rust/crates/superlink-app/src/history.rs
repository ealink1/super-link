//! Execution history. Only redacted statements are persisted: literal values
//! never reach the database.

use std::sync::Arc;

use superlink_domain::History;
use superlink_infra::Store;
use superlink_sql::lexer::{TokenKind, tokenize};

const MAX_HISTORY_TEXT: usize = 4 << 10;
const DEFAULT_LIMIT: usize = 50;

pub struct HistoryService {
    store: Arc<Store>,
}

impl HistoryService {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    pub fn list(&self, profile_id: &str) -> Vec<History> {
        self.store
            .history(profile_id, DEFAULT_LIMIT)
            .unwrap_or_default()
    }

    /// Returns the row id so the caller can remember the original text for
    /// this session; only the redacted copy is persisted.
    pub fn record(
        &self,
        profile_id: &str,
        sql: &str,
        success: bool,
        rows: i64,
        duration_ns: u64,
    ) -> Option<i64> {
        let entry = History {
            profile_id: profile_id.to_string(),
            text: redact(sql),
            success,
            rows,
            duration_ns,
            ..Default::default()
        };
        self.store.add_history(&entry).ok()
    }

    pub fn clear(&self, profile_id: &str) {
        let _ = self.store.clear_history(profile_id);
    }
}

/// Replaces string and numeric literals with a placeholder and bounds the
/// stored length. Comments are dropped for the same reason.
pub fn redact(sql: &str) -> String {
    let mut output = String::with_capacity(sql.len().min(MAX_HISTORY_TEXT));
    for token in tokenize(sql) {
        let text = match token.kind {
            TokenKind::String | TokenKind::Number => "?",
            TokenKind::Comment => continue,
            _ => token.text.as_str(),
        };
        if output.len() + text.len() > MAX_HISTORY_TEXT {
            output.push('…');
            break;
        }
        output.push_str(text);
    }
    output.trim().to_string()
}

/// Number of literals that were replaced by the redaction marker.
pub fn placeholder_count(text: &str) -> usize {
    tokenize(text)
        .into_iter()
        .filter(|token| token.kind == TokenKind::Plain && token.text == "?")
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_are_replaced() {
        let sql = "SELECT * FROM users WHERE name = 'alice' AND age > 30";
        let redacted = redact(sql);
        assert!(!redacted.contains("alice"));
        assert!(!redacted.contains("30"));
        assert!(redacted.contains("SELECT"));
        assert!(redacted.contains("?"));
    }

    #[test]
    fn counts_placeholders_in_redacted_text() {
        let redacted = redact("SELECT * FROM t WHERE a = 'x' AND b = 2");
        assert_eq!(placeholder_count(&redacted), 2);
        assert_eq!(placeholder_count("SELECT 1"), 0);
    }

    #[test]
    fn comments_are_dropped() {
        let redacted = redact("SELECT 1 -- secret note\nFROM dual");
        assert!(!redacted.contains("secret"));
        assert!(redacted.contains("FROM dual"));
    }

    #[test]
    fn text_is_bounded() {
        let sql = format!("SELECT '{}'", "x".repeat(8 << 10));
        assert!(redact(&sql).chars().count() <= MAX_HISTORY_TEXT + 1);
    }

    #[test]
    fn history_round_trips_and_clears() {
        let dir = std::env::temp_dir().join(format!("superlink-history-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Arc::new(Store::open(dir.join("state.db")).unwrap());
        store
            .save_profile(&superlink_domain::Profile {
                id: "p1".into(),
                ..Default::default()
            })
            .unwrap();
        let service = HistoryService::new(store);
        service.record("p1", "SELECT 'secret'", true, 3, 1_500_000);
        let list = service.list("p1");
        assert_eq!(list.len(), 1);
        let id = service.record("p1", "SELECT 2", true, 0, 0).expect("id");
        assert!(id > list[0].id);
        assert_eq!(service.list("p1").len(), 2);
        assert!(!list[0].text.contains("secret"));
        assert_eq!(list[0].rows, 3);
        service.clear("p1");
        assert!(service.list("p1").is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
