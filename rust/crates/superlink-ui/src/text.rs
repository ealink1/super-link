//! 文本展示辅助。连接名等用户输入没有长度上限，展示时按字符数截断。

/// 双击选词时视为单词一部分的字符：标识符与常见数字写法。
fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte == b'.'
}

/// `offset` 处单词的字节范围。
///
/// 光标落在单词字符上时选该词；落在分隔符上时选左侧的词（双击词尾后的
/// 空格、逗号等仍选中刚点过的词），这一行为与 macOS 文本控件一致。
pub fn word_range(text: &str, offset: usize) -> std::ops::Range<usize> {
    let bytes = text.as_bytes();
    let offset = offset.min(text.len());
    let anchor = if bytes.get(offset).is_some_and(|&b| is_word_byte(b)) {
        offset
    } else if offset > 0 && is_word_byte(bytes[offset - 1]) {
        offset - 1
    } else {
        // 两侧都不是单词字符时，向右找第一个词；找不到就返回空范围。
        match (offset..bytes.len()).find(|&i| is_word_byte(bytes[i])) {
            Some(i) => i,
            None => return offset..offset,
        }
    };
    let mut start = anchor;
    while start > 0 && is_word_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = anchor;
    while end < bytes.len() && is_word_byte(bytes[end]) {
        end += 1;
    }
    start..end
}

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
    use super::{shorten, word_range};

    #[test]
    fn word_range_covers_the_identifier_under_the_cursor() {
        let text = "SELECT * FROM `ott_inf`.`sys_menus` LIMIT 50";
        let offset = text.find("sys_menus").unwrap() + 3;
        // 反引号属于分隔符，双击表名只选中表名本身。
        assert_eq!(&text[word_range(text, offset)], "sys_menus");
    }

    #[test]
    fn word_range_stops_at_spaces_and_punctuation() {
        let text = "a, bc";
        assert_eq!(&text[word_range(text, 0)], "a");
        assert_eq!(&text[word_range(text, 1)], "a");
        assert_eq!(&text[word_range(text, 3)], "bc");
        // 逗号之后的空格两侧都不是单词字符，交给右侧的单词。
        assert_eq!(&text[word_range(text, 2)], "bc");
    }

    #[test]
    fn word_range_skips_leading_separators() {
        let text = "  select 1";
        // 光标落在前导空格里时选右侧第一个词。
        assert_eq!(&text[word_range(text, 0)], "select");
        assert_eq!(&text[word_range(text, 1)], "select");
    }

    #[test]
    fn word_range_clamps_to_text_bounds() {
        let text = "name";
        assert_eq!(&text[word_range(text, 0)], "name");
        assert_eq!(&text[word_range(text, 4)], "name");
        // 纯分隔符文本命中不到单词，返回空范围。
        assert!(word_range("   ", 1).is_empty());
    }

    #[test]
    fn word_range_keeps_qualified_names_together() {
        let text = "select ott.member from t";
        let offset = text.find("member").unwrap();
        assert_eq!(&text[word_range(text, offset)], "ott.member");
    }

    #[test]
    fn word_range_selects_the_right_side_after_a_space() {
        let text = "select name from t";
        // 双击词尾后的空格：按左侧单词处理。
        let space = text.find(" name").unwrap() + 4;
        assert_eq!(&text[word_range(text, space)], "name");
        // 光标直接落在单词起点时选中整词。
        let offset = text.find("name").unwrap();
        assert_eq!(&text[word_range(text, offset)], "name");
    }

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
