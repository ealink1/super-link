//! UTF-8 source positions for masked fields and native IME composition.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub(super) fn display_offset(text: &str, masked: bool, byte: usize) -> usize {
    if masked {
        text.grapheme_indices(true)
            .take_while(|(i, _)| *i < byte)
            .count()
    } else {
        byte
    }
}
pub(super) fn source_offset(text: &str, masked: bool, display: usize) -> usize {
    if masked {
        return text
            .grapheme_indices(true)
            .nth(display)
            .map(|(i, _)| i)
            .unwrap_or(text.len());
    }
    let mut byte = display.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}
fn utf16_byte(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units >= offset {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}
pub(super) fn composition_selection(
    start: usize,
    text: &str,
    selected: Range<usize>,
) -> Range<usize> {
    start + utf16_byte(text, selected.start)..start + utf16_byte(text, selected.end)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composition_offsets_are_relative_to_new_text() {
        assert_eq!(composition_selection(7, "中文😀", 1..4), 10..17);
    }
    #[test]
    fn masked_clicks_and_carets_map_whole_graphemes() {
        let text = "中e\u{301}😀";
        assert_eq!(display_offset(text, true, text.len()), 3);
        assert_eq!(source_offset(text, true, 2), 6);
        assert_eq!(source_offset(text, true, 3), text.len());
        assert_eq!(source_offset(text, false, 2), 0);
    }
}
