//! 文本展示辅助。连接名等用户输入没有长度上限，展示时按字符数截断。

/// 超过 `max_chars` 个字符时截断并追加省略号；未超出则原样返回。
pub fn shorten(text: &str, max_chars: usize) -> String {
    let mut result = String::with_capacity(text.len());
    for (index, ch) in text.chars().enumerate() {
        if index == max_chars {
            result.push('…');
            break;
        }
        result.push(ch);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::shorten;

    #[test]
    fn keeps_short_text() {
        assert_eq!(shorten("本地 MySQL", 22), "本地 MySQL");
    }

    #[test]
    fn keeps_exact_length() {
        assert_eq!(shorten("abcdef", 6), "abcdef");
    }

    #[test]
    fn truncates_long_text() {
        assert_eq!(shorten("abcdefgh", 6), "abcdef…");
    }

    #[test]
    fn counts_characters_not_bytes() {
        assert_eq!(shorten("数据库连接名称超长测试", 4), "数据库连…");
    }
}
