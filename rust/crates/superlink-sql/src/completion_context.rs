//! Completion context analysis: which token the cursor sits in, which tables
//! the statement already references, and whether completion is suppressed.
//!
//! Offsets are character (rune) offsets, matching the editor cursor; the size
//! guard uses bytes, matching the Go implementation.

use crate::lexer::is_keyword;

/// Everything the candidate builder needs to know about the cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompletionQuery {
    pub start: usize,
    pub end: usize,
    pub prefix: String,
    pub qualifier: String,
    /// Opening quote of the identifier under the cursor, if any.
    pub quote: Option<char>,
    pub table_position: bool,
    pub active: bool,
    pub references: Vec<CompletionReference>,
}

/// One table mentioned by the current statement.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompletionReference {
    pub schema: String,
    pub name: String,
    pub alias: String,
}

#[derive(Debug, Clone)]
struct CompletionToken {
    text: String,
    start: usize,
    end: usize,
    quote: Option<char>,
    blocked: bool,
    closed: bool,
}

const MAX_SOURCE_BYTES: usize = 256 << 10;

/// Avoids strings/comments and bounds work on very large scripts.
pub fn analyze_completion(source: &str, cursor: usize) -> CompletionQuery {
    let mut query = CompletionQuery {
        start: cursor,
        end: cursor,
        ..Default::default()
    };
    if source.len() > MAX_SOURCE_BYTES {
        return query;
    }
    let characters: Vec<char> = source.chars().collect();
    if cursor > characters.len() {
        return query;
    }
    let tokens = completion_tokens(&characters);
    let mut before: Vec<CompletionToken> = Vec::new();
    let mut statement: Vec<CompletionToken> = Vec::new();
    for token in &tokens {
        if token.blocked {
            if cursor > token.start
                && (cursor < token.end || (cursor == token.end && !token.closed))
            {
                return query;
            }
            continue;
        }
        if token.text == ";" {
            if token.end <= cursor {
                before.clear();
                statement.clear();
                continue;
            }
            break;
        }
        statement.push(token.clone());
        if cursor > token.start
            && cursor <= token.end
            && (completion_word(characters[token.start]) || token.quote.is_some())
        {
            query.start = token.start;
            query.end = token.end;
            query.quote = token.quote;
            let mut start = token.start;
            let mut end = cursor;
            if token.quote.is_some() {
                start += 1;
                if token.closed && end == token.end {
                    end -= 1;
                }
            }
            query.prefix = characters[start..end].iter().collect();
        } else if token.end <= cursor {
            before.push(token.clone());
        }
    }
    if query.start > 0 {
        let previous = characters[query.start - 1];
        if matches!(previous, ':' | '@' | '$') {
            return query;
        }
    }
    if query.start > 1 && characters[query.start - 2] == '$' && characters[query.start - 1] == '{' {
        return query;
    }
    if let Some(last) = before.last() {
        if last.text == "." {
            before.pop();
            if let Some(previous) = before.pop() {
                query.qualifier = previous.text;
            }
        }
    }
    if let Some(last) = before.last() {
        let upper = last.text.to_uppercase();
        query.table_position = matches!(
            upper.as_str(),
            "FROM" | "JOIN" | "UPDATE" | "INTO" | "TABLE"
        );
    }
    query.references = completion_references(&statement);
    query.active = query.end - query.start <= 512;
    query
}

fn completion_word(character: char) -> bool {
    character.is_alphabetic() || character.is_numeric() || character == '_' || character == '$'
}

fn completion_tokens(source: &[char]) -> Vec<CompletionToken> {
    let mut tokens: Vec<CompletionToken> = Vec::with_capacity(source.len().min(256));
    let mut index = 0;
    while index < source.len() && tokens.len() < 20_000 {
        if source[index].is_whitespace() {
            index += 1;
            continue;
        }
        let start = index;
        let mut token = CompletionToken {
            text: String::new(),
            start,
            end: start,
            quote: None,
            blocked: false,
            closed: true,
        };
        let current = source[index];
        index += 1;
        match current {
            '-' if index < source.len() && source[index] == '-' => {
                token.blocked = true;
                while index < source.len() && source[index] != '\n' {
                    index += 1;
                }
                token.closed = index < source.len();
            }
            '/' if index < source.len() && source[index] == '*' => {
                token.blocked = true;
                index += 1;
                let mut depth = 1;
                while index < source.len() && depth > 0 {
                    if index + 1 < source.len() && source[index] == '/' && source[index + 1] == '*'
                    {
                        depth += 1;
                        index += 2;
                    } else if index + 1 < source.len()
                        && source[index] == '*'
                        && source[index + 1] == '/'
                    {
                        depth -= 1;
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
                token.closed = depth == 0;
            }
            '\'' | '"' | '\x60' | '[' => {
                token.blocked = current == '\'';
                token.closed = false;
                token.quote = Some(current);
                let end_character = if current == '[' { ']' } else { current };
                while index < source.len() {
                    if source[index] == '\\' && current == '\'' && index + 1 < source.len() {
                        index += 2;
                    } else if source[index] == end_character {
                        index += 1;
                        if index < source.len() && source[index] == end_character {
                            index += 1;
                            continue;
                        }
                        token.closed = true;
                        break;
                    } else {
                        index += 1;
                    }
                }
            }
            '$' => {
                let mut probe = index;
                while probe < source.len()
                    && (source[probe].is_alphabetic()
                        || source[probe].is_numeric()
                        || source[probe] == '_')
                {
                    probe += 1;
                }
                if probe < source.len() && source[probe] == '$' {
                    let delimiter: Vec<char> = source[start..=probe].to_vec();
                    token.blocked = true;
                    token.closed = false;
                    index = probe + 1;
                    while index < source.len() {
                        if source[index..].starts_with(&delimiter) {
                            index += delimiter.len();
                            token.closed = true;
                            break;
                        }
                        index += 1;
                    }
                }
            }
            _ if completion_word(current) => {
                while index < source.len() && completion_word(source[index]) {
                    index += 1;
                }
            }
            _ => {}
        }
        token.end = index;
        token.text = source[token.start..index].iter().collect();
        if token.quote.is_some() && !token.blocked {
            let mut end = index;
            if token.closed {
                end -= 1;
            }
            token.text = source[token.start + 1..end].iter().collect();
        }
        tokens.push(token);
    }
    tokens
}

fn completion_references(tokens: &[CompletionToken]) -> Vec<CompletionReference> {
    let mut references = Vec::new();
    let mut index = 0;
    while index + 1 < tokens.len() && references.len() < 16 {
        let keyword = tokens[index].text.to_uppercase();
        if !matches!(keyword.as_str(), "FROM" | "JOIN" | "UPDATE" | "INTO")
            || tokens[index].quote.is_some()
        {
            index += 1;
            continue;
        }
        index += 1;
        if tokens[index].text == "(" {
            index += 1;
            continue;
        }
        let mut reference = CompletionReference {
            name: tokens[index].text.clone(),
            ..Default::default()
        };
        if index + 2 < tokens.len() && tokens[index + 1].text == "." {
            reference.schema = std::mem::take(&mut reference.name);
            reference.name = tokens[index + 2].text.clone();
            index += 2;
        }
        if index + 1 < tokens.len() && tokens[index + 1].text.eq_ignore_ascii_case("AS") {
            index += 1;
        }
        if index + 1 < tokens.len() && !tokens[index + 1].text.is_empty() {
            let next = &tokens[index + 1];
            let starts_word = next
                .text
                .chars()
                .next()
                .map(completion_word)
                .unwrap_or(false);
            if next.quote.is_some() || (!is_keyword(&next.text) && starts_word) {
                reference.alias = next.text.clone();
            }
        }
        references.push(reference);
        index += 1;
    }
    references
}
