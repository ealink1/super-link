//! Display-only SQL tokenization.
//!
//! The lexer never rewrites text: concatenating every token reproduces the
//! source character for character, including comments, whitespace and
//! unfinished edits.

use std::collections::HashSet;
use std::sync::LazyLock;

/// How one display token should be highlighted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Plain,
    Keyword,
    String,
    Number,
    Comment,
}

impl TokenKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            TokenKind::Plain => "plain",
            TokenKind::Keyword => "keyword",
            TokenKind::String => "string",
            TokenKind::Number => "number",
            TokenKind::Comment => "comment",
        }
    }
}

/// One bounded display token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub kind: TokenKind,
}

const LEXER_KEYWORDS: &str = "SELECT FROM WHERE AS AND OR NOT NULL IS IN LIKE BETWEEN EXISTS JOIN LEFT RIGHT INNER OUTER FULL CROSS ON USING WITH UNION ALL DISTINCT ORDER BY ASC DESC GROUP HAVING LIMIT OFFSET FETCH ROWS NEXT ONLY INSERT INTO VALUES UPDATE SET DELETE CREATE ALTER DROP TABLE VIEW INDEX PRIMARY KEY REFERENCES DEFAULT RETURNING CASE WHEN THEN ELSE END TRUE FALSE CAST COALESCE COUNT SUM AVG MIN MAX BEGIN COMMIT ROLLBACK EXPLAIN SHOW DESCRIBE";

/// Keywords offered by completion; the order is the candidate order.
pub(crate) const COMPLETION_KEYWORDS: &str = "SELECT FROM WHERE JOIN LEFT RIGHT INNER OUTER ON AS AND OR NOT NULL IS IN LIKE BETWEEN EXISTS ORDER BY GROUP HAVING LIMIT OFFSET DISTINCT COUNT SUM AVG MIN MAX INSERT INTO VALUES UPDATE SET DELETE CREATE ALTER DROP TABLE VIEW INDEX PRIMARY KEY REFERENCES DEFAULT RETURNING CASE WHEN THEN ELSE END TRUE FALSE CAST COALESCE UNION ALL WITH BEGIN COMMIT ROLLBACK EXPLAIN SHOW DESCRIBE";

static KEYWORDS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| LEXER_KEYWORDS.split_whitespace().collect());

/// True when the word (in any case) is a reserved word we highlight.
pub fn is_keyword(word: &str) -> bool {
    KEYWORDS.contains(word.to_uppercase().as_str())
}

const MAX_TOKENS: usize = 10_000;

/// Splits source into display tokens while preserving every character.
pub fn tokenize(text: &str) -> Vec<Token> {
    let characters: Vec<char> = text.chars().collect();
    let mut tokens: Vec<Token> = Vec::with_capacity(characters.len().min(256));
    let mut index = 0;
    while index < characters.len() {
        if tokens.len() >= MAX_TOKENS {
            tokens.push(Token {
                text: characters[index..].iter().collect(),
                kind: TokenKind::Plain,
            });
            break;
        }
        let start = index;
        let mut kind = TokenKind::Plain;
        let current = characters[index];
        index += 1;
        match current {
            '-' if index < characters.len() && characters[index] == '-' => {
                kind = TokenKind::Comment;
                while index < characters.len() && characters[index] != '\n' {
                    index += 1;
                }
            }
            '/' if index < characters.len() && characters[index] == '*' => {
                kind = TokenKind::Comment;
                index += 1;
                let mut depth = 1;
                while index < characters.len() && depth > 0 {
                    if characters[index] == '/'
                        && index + 1 < characters.len()
                        && characters[index + 1] == '*'
                    {
                        depth += 1;
                        index += 2;
                    } else if characters[index] == '*'
                        && index + 1 < characters.len()
                        && characters[index + 1] == '/'
                    {
                        depth -= 1;
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
            }
            '\'' | '"' | '\x60' => {
                if current == '\'' {
                    kind = TokenKind::String;
                }
                while index < characters.len() {
                    if characters[index] == '\\' && index + 1 < characters.len() {
                        index += 2;
                        continue;
                    }
                    if characters[index] == current {
                        index += 1;
                        if index < characters.len() && characters[index] == current {
                            index += 1;
                            continue;
                        }
                        break;
                    }
                    index += 1;
                }
            }
            _ if current.is_alphabetic() || current == '_' => {
                while index < characters.len()
                    && (characters[index].is_alphanumeric() || characters[index] == '_')
                {
                    index += 1;
                }
                let word: String = characters[start..index].iter().collect();
                if is_keyword(&word) {
                    kind = TokenKind::Keyword;
                }
            }
            _ if current.is_numeric() => {
                kind = TokenKind::Number;
                while index < characters.len()
                    && (characters[index].is_numeric() || characters[index] == '.')
                {
                    index += 1;
                }
            }
            _ if current.is_whitespace() => {
                while index < characters.len() && characters[index].is_whitespace() {
                    index += 1;
                }
            }
            _ => {}
        }
        tokens.push(Token {
            text: characters[start..index].iter().collect(),
            kind,
        });
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rebuild(source: &str) -> String {
        tokenize(source)
            .into_iter()
            .map(|token| token.text)
            .collect::<String>()
    }

    #[test]
    fn preserves_incomplete_source_and_whitespace() {
        let long = format!("SELECT {}", "a,".repeat(15_000));
        for source in [
            "SELECT 1 AS id, 'Fyne parity' AS name, NULL;",
            "-- 中文\nSELECT \"a.b\" FROM \"表\";",
            "/* nested /* comment */ */\nSELECT 'unclosed",
            long.as_str(),
        ] {
            assert_eq!(rebuild(source), source);
        }
    }

    #[test]
    fn classifies_each_token_kind() {
        let tokens = tokenize("SELECT 'a' 42 -- end");
        let mut counts = std::collections::HashMap::new();
        for token in &tokens {
            *counts.entry(token.kind).or_insert(0) += 1;
        }
        for kind in [
            TokenKind::Keyword,
            TokenKind::String,
            TokenKind::Number,
            TokenKind::Comment,
        ] {
            assert_eq!(counts.get(&kind), Some(&1), "{kind:?} {tokens:?}");
        }
        assert_eq!(TokenKind::Keyword.as_str(), "keyword");
        assert!(is_keyword("select"));
        assert!(!is_keyword("items"));
    }
}
