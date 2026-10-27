//! Completion candidates built from a bounded metadata snapshot.

use std::collections::HashSet;

use crate::completion_context::CompletionQuery;
use crate::identifier::quote_identifier;
use crate::lexer::{COMPLETION_KEYWORDS, is_keyword};

/// One table from the cached metadata snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompletionTable {
    pub schema: String,
    pub name: String,
    pub columns: Vec<String>,
}

/// One item offered by the editor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompletionCandidate {
    pub text: String,
    pub insert: String,
    pub kind: String,
    pub detail: String,
}

const MAX_RESULTS: usize = 60;
const MAX_TABLES: usize = 5000;
const MAX_COLUMNS: usize = 2000;

/// Completes from a snapshot only; it performs no database I/O.
pub fn complete(
    query: &CompletionQuery,
    tables: &[CompletionTable],
    dialect: &str,
) -> Vec<CompletionCandidate> {
    if !query.active {
        return Vec::new();
    }
    let mut result: Vec<CompletionCandidate> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut ordered: Vec<&CompletionTable> = tables.iter().take(MAX_TABLES).collect();
    ordered.sort_by(|left, right| left.name.cmp(&right.name));

    if !query.table_position {
        for reference in &query.references {
            if !query.qualifier.is_empty()
                && !equal_fold(&query.qualifier, &reference.alias)
                && !equal_fold(&query.qualifier, &reference.name)
            {
                continue;
            }
            for table in &ordered {
                if !equal_fold(&table.name, &reference.name)
                    || (!reference.schema.is_empty()
                        && !equal_fold(&table.schema, &reference.schema))
                {
                    continue;
                }
                for column in table.columns.iter().take(MAX_COLUMNS) {
                    add(
                        &mut result,
                        &mut seen,
                        query,
                        dialect,
                        column,
                        "字段",
                        &table.name,
                        true,
                    );
                }
            }
        }
    }
    if query.qualifier.is_empty() && !query.table_position && query.quote.is_none() {
        for word in COMPLETION_KEYWORDS.split_whitespace() {
            add(
                &mut result,
                &mut seen,
                query,
                dialect,
                word,
                "关键字",
                "SQL",
                false,
            );
        }
    }
    for table in &ordered {
        if !query.qualifier.is_empty() && !equal_fold(&query.qualifier, &table.schema) {
            continue;
        }
        add(
            &mut result,
            &mut seen,
            query,
            dialect,
            &table.name,
            "表",
            &table.schema,
            true,
        );
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn add(
    result: &mut Vec<CompletionCandidate>,
    seen: &mut HashSet<String>,
    query: &CompletionQuery,
    dialect: &str,
    name: &str,
    kind: &str,
    detail: &str,
    identifier: bool,
) {
    if result.len() >= MAX_RESULTS || name.len() > 512 || !starts_with_fold(name, &query.prefix) {
        return;
    }
    let insert = if identifier {
        completion_identifier(name, dialect, query.quote)
    } else if !query.prefix.is_empty() && query.prefix == query.prefix.to_lowercase() {
        name.to_lowercase()
    } else {
        name.to_string()
    };
    if insert.is_empty() || seen.contains(&insert) {
        return;
    }
    if query.quote.is_none()
        && insert == query.prefix
        && query.end.saturating_sub(query.start) == query.prefix.chars().count()
    {
        return;
    }
    seen.insert(insert.clone());
    result.push(CompletionCandidate {
        text: name.to_string(),
        insert,
        kind: kind.to_string(),
        detail: detail.to_string(),
    });
}

fn equal_fold(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}

fn starts_with_fold(value: &str, prefix: &str) -> bool {
    value.to_lowercase().starts_with(&prefix.to_lowercase())
}

/// Quotes only when the name needs it for the active dialect.
pub fn completion_identifier(name: &str, dialect: &str, quote: Option<char>) -> String {
    if let Some(opening) = quote {
        let closing = if opening == '[' { ']' } else { opening };
        return format!(
            "{opening}{}{closing}",
            name.replace(closing, &closing.to_string().repeat(2))
        );
    }
    let mut plain = !is_keyword(name);
    for (index, character) in name.chars().enumerate() {
        plain = plain
            && (character.is_alphabetic()
                || character == '_'
                || (index > 0 && (character.is_numeric() || character == '$')));
    }
    // PostgreSQL folds unquoted names to lowercase.
    if dialect == "postgres" && name != name.to_lowercase() {
        plain = false;
    }
    if plain {
        return name.to_string();
    }
    quote_identifier(dialect, name).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::completion_context::analyze_completion;

    fn completion_at(source: &str) -> (String, CompletionQuery) {
        let (before, after) = source.split_once('|').expect("marker");
        let text = format!("{before}{after}");
        let cursor = before.chars().count();
        let query = analyze_completion(&text, cursor);
        (text, query)
    }

    fn tables() -> Vec<CompletionTable> {
        vec![
            CompletionTable {
                schema: "public".into(),
                name: "users".into(),
                columns: vec!["id".into(), "name".into(), "昵称".into(), "order".into()],
            },
            CompletionTable {
                schema: "public".into(),
                name: "user_logs".into(),
                columns: vec!["event".into()],
            },
            CompletionTable {
                schema: "other".into(),
                name: "payments".into(),
                columns: vec!["amount".into()],
            },
        ]
    }

    #[test]
    fn uses_sql_context_and_character_ranges() {
        let cases = [
            ("se|", "select", "关键字"),
            ("SELECT * FROM us|", "users", "表"),
            ("SELECT * FROM public.us|", "users", "表"),
            ("SELECT u.na| FROM users u", "name", "字段"),
            ("SELECT u.昵| FROM public.users AS u", "昵称", "字段"),
            ("SELECT na| FROM users", "name", "字段"),
            ("SELECT u.or| FROM users u", "\"order\"", "字段"),
            ("SELECT \"na|\" FROM users", "\"name\"", "字段"),
            (
                "SELECT * FROM users u JOIN payments p ON p.am|",
                "amount",
                "字段",
            ),
        ];
        for (source, want, kind) in cases {
            let (text, query) = completion_at(source);
            let items = complete(&query, &tables(), "postgres");
            let found = items
                .iter()
                .find(|item| item.insert == want && item.kind == kind);
            assert!(
                found.is_some(),
                "missing {want:?} ({kind}) for {query:?}: {items:?}"
            );
            let characters: Vec<char> = text.chars().collect();
            let replaced = format!(
                "{}{}{}",
                characters[..query.start].iter().collect::<String>(),
                found.unwrap().insert,
                characters[query.end..].iter().collect::<String>()
            );
            assert!(!replaced.contains("\"name\"\""), "{replaced}");
            if query.table_position {
                assert!(items.iter().all(|item| item.kind == "表"), "{items:?}");
            }
        }
    }

    #[test]
    fn suppresses_strings_comments_and_parameters() {
        for source in [
            "SELECT 'se|",
            "SELECT 'it''s se| here'",
            "-- SELECT se|",
            "/* outer /* nested */ se| */",
            "SELECT $body$ SELECT se| $body$",
            "SELECT $$ se|",
            "SELECT :na|",
            "SELECT @na|",
            "SELECT ${na|}",
        ] {
            let (_, query) = completion_at(source);
            assert!(
                !query.active,
                "completion active inside non-code: {source:?}"
            );
        }
        let (_, query) = completion_at("-- comment\nse|");
        assert!(query.active);
        assert_eq!(query.prefix, "se");
    }

    #[test]
    fn replaces_the_whole_token_and_ignores_other_statements() {
        let (text, query) =
            completion_at("SELECT '中文'; SELECT u.na|me FROM users u; SELECT * FROM payments p");
        let characters: Vec<char> = text.chars().collect();
        assert_eq!(
            characters[query.start..query.end]
                .iter()
                .collect::<String>(),
            "name"
        );
        assert_eq!(query.prefix, "na");
        assert_eq!(query.references.len(), 1);
        assert_eq!(query.references[0].name, "users");
        let huge = format!("{}se", " ".repeat(256 << 10));
        assert!(!analyze_completion(&huge, 2).active);
    }

    #[test]
    fn quotes_metadata_and_limits_results() {
        for (dialect, want) in [
            ("mysql", "\x60order\x60"),
            ("sqlserver", "[order]"),
            ("postgres", "\"order\""),
        ] {
            let (_, query) = completion_at("SELECT * FROM or|");
            let items = complete(
                &query,
                &[CompletionTable {
                    name: "order".into(),
                    ..Default::default()
                }],
                dialect,
            );
            assert_eq!(items.len(), 1, "{dialect}");
            assert_eq!(items[0].insert, want);
        }
        let wide: Vec<CompletionTable> = (0..6000)
            .map(|index| CompletionTable {
                name: "x".repeat(index % 100 + 1),
                ..Default::default()
            })
            .collect();
        let (_, query) = completion_at("SELECT * FROM |");
        assert_eq!(complete(&query, &wide, "mysql").len(), MAX_RESULTS);
    }
}
