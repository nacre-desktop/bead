// SPDX-License-Identifier: MIT OR Apache-2.0
//! Basic Unicode editing. Cursor/selection offsets are UTF-8 boundaries.
use unicode_segmentation::UnicodeSegmentation;
#[derive(Clone, Copy, Debug)]
pub enum EditKey {
    Left,
    Right,
    Home,
    End,
    Delete,
    SelectAll,
}
/// One bullet per grapheme; a masked TextInput draws this instead of its value.
pub const MASK: char = '\u{2022}';
pub fn mask(text: &str) -> String {
    text.graphemes(true).map(|_| MASK).collect()
}
/// Byte offset in `mask(text)` for the byte offset `at` of `text`.
pub fn mask_offset(text: &str, at: usize) -> usize {
    text.grapheme_indices(true)
        .take_while(|(i, _)| *i < at)
        .count()
        * MASK.len_utf8()
}
pub fn boundaries(text: &str) -> Vec<usize> {
    let mut values: Vec<_> = text.grapheme_indices(true).map(|(i, _)| i).collect();
    values.push(text.len());
    values
}
pub fn replace(text: &str, cursor: usize, anchor: usize, insert: &str) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let anchor = anchor.min(text.len());
    let start = cursor.min(anchor);
    let end = cursor.max(anchor);
    let mut result = String::with_capacity(text.len() + insert.len());
    result.push_str(&text[..start]);
    result.push_str(insert);
    result.push_str(&text[end..]);
    (result, start + insert.len())
}
pub fn move_cursor(text: &str, cursor: usize, key: EditKey) -> usize {
    let points = boundaries(text);
    match key {
        EditKey::Home | EditKey::SelectAll => 0,
        EditKey::End => text.len(),
        EditKey::Left => points.into_iter().rfind(|&p| p < cursor).unwrap_or(0),
        _ => points
            .into_iter()
            .find(|&p| p > cursor)
            .unwrap_or(text.len()),
    }
}
#[derive(Clone, Default)]
pub struct Editor {
    pub cursor: usize,
    pub anchor: usize,
    pub preedit: String,
}
impl Editor {
    pub fn new(length: usize) -> Self {
        Self {
            cursor: length,
            anchor: length,
            preedit: String::new(),
        }
    }
    pub fn clamp(&mut self, text: &str) {
        let points = boundaries(text);
        self.cursor = points
            .iter()
            .copied()
            .rfind(|&p| p <= self.cursor)
            .unwrap_or(0);
        self.anchor = points.into_iter().rfind(|&p| p <= self.anchor).unwrap_or(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graphemes_and_selection_are_not_split() {
        let text = "A👩‍🚀é";
        let left = move_cursor(text, text.len(), EditKey::Left);
        assert_eq!(&text[left..], "é");
        let first = move_cursor(text, left, EditKey::Left);
        assert_eq!(&text[first..left], "👩‍🚀");
        let (edited, pos) = replace(text, first, left, "😀");
        assert_eq!(edited, "A😀é");
        assert!(edited.is_char_boundary(pos));
    }
}
