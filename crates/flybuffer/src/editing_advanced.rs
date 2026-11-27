use crate::{SubString, TextBuffer, WordDelim};
use unicode_segmentation::UnicodeSegmentation;

impl TextBuffer {
    fn less_strict_class(g: &str) -> u8 {
        match g.chars().next() {
            Some(c) if c.is_whitespace() => 0,
            Some(c) if c.is_ascii_punctuation() => 1,
            _ => 2,
        }
    }

    fn less_strict_class_slash_only(g: &str) -> u8 {
        match g.chars().next() {
            Some(c) if c.is_whitespace() => 0,
            Some('/' | '\\') => 1,
            _ => 2,
        }
    }

    fn has_slash_in_word(buf: &str, cursor_byte: usize) -> bool {
        let left = buf[..cursor_byte]
            .chars()
            .rev()
            .take_while(|c| !c.is_whitespace())
            .any(|c| c == '/' || c == '\\');
        let right = buf[cursor_byte..]
            .chars()
            .take_while(|c| !c.is_whitespace())
            .any(|c| c == '/' || c == '\\');
        left || right
    }

    pub fn delete_left(&mut self) {
        // delete one grapheme to the left
        self.push_snapshot(true);
        let old_cursor_col = self.cursor_byte;
        self.move_left();
        assert!(self.cursor_byte <= old_cursor_col);
        self.buf.drain(self.cursor_byte..old_cursor_col);
        self.cursor_byte = Self::snap_to_grapheme_boundary_left(&self.buf, self.cursor_byte);
        self.selection_byte = None;
    }

    pub fn delete_right(&mut self) {
        // delete one grapheme to the right
        self.push_snapshot(true);
        self.selection_byte = None;
        let cursor_pos_right = self.right_move_pos();
        assert!(self.cursor_byte <= cursor_pos_right);
        self.buf.drain(self.cursor_byte..cursor_pos_right);
        self.cursor_byte = Self::snap_to_grapheme_boundary_left(&self.buf, self.cursor_byte);
    }

    /// Computes the target cursor byte position when moving/deleting one
    /// fine-grained word to the left (stopping at punctuation or path-segment
    /// boundaries, with slash-only mode when `slash_aware` is true and the word
    /// under the cursor contains `/` or `\`).
    pub(crate) fn fine_grained_word_left_pos_from(
        &self,
        cursor_byte: usize,
        slash_aware: bool,
    ) -> usize {
        let class_fn: fn(&str) -> u8 =
            if slash_aware && Self::has_slash_in_word(&self.buf, cursor_byte) {
                Self::less_strict_class_slash_only
            } else {
                Self::less_strict_class
            };
        let mut iter = self
            .buf
            .grapheme_indices(true)
            .rev()
            .skip_while(|(i, _)| *i >= cursor_byte);
        match iter.next() {
            Some((first_i, first_g)) => {
                let class = class_fn(first_g);
                iter.scan((first_i, first_g), |prev, (i, g)| {
                    let (prev_i, prev_g) = *prev;
                    let boundary = if class_fn(prev_g) == class && class_fn(g) != class {
                        Some(prev_i)
                    } else {
                        None
                    };
                    *prev = (i, g);
                    Some(boundary)
                })
                .find_map(|x| x)
                .unwrap_or(0)
            }
            None => 0,
        }
    }

    pub(crate) fn fine_grained_word_left_pos(&self, slash_aware: bool) -> usize {
        self.fine_grained_word_left_pos_from(self.cursor_byte, slash_aware)
    }

    /// Computes the target cursor byte position when moving/deleting one
    /// fine-grained word to the right (stopping at punctuation or path-segment
    /// boundaries, with slash-only mode when `slash_aware` is true and the word
    /// under the cursor contains `/` or `\`).
    pub(crate) fn fine_grained_word_right_pos_from(
        &self,
        cursor_byte: usize,
        slash_aware: bool,
    ) -> usize {
        fine_grained_word_right_pos_in(&self.buf, cursor_byte, slash_aware)
    }

    pub(crate) fn fine_grained_word_right_pos(&self, slash_aware: bool) -> usize {
        self.fine_grained_word_right_pos_from(self.cursor_byte, slash_aware)
    }

    pub fn delete_one_word_left(&mut self, delim: WordDelim) {
        self.push_snapshot(true);
        self.selection_byte = None;
        let old_cursor_col = self.cursor_byte;

        // First, find the position reached by skipping back over any contiguous
        // run of whitespace immediately before the cursor.
        let after_ws_skip = self.buf[..old_cursor_col]
            .grapheme_indices(true)
            .rev()
            .find(|(_, g)| !g.chars().next().is_some_and(char::is_whitespace))
            .map_or(0, |(i, g)| i + g.len());
        let ws_graphemes = self.buf[after_ws_skip..old_cursor_col]
            .graphemes(true)
            .count();

        // If there are 2+ contiguous whitespace graphemes before the cursor, just
        // delete the whitespace and stop. Otherwise (0 or 1 ws graphemes), also
        // consume the previous word using the per-delim word-boundary logic.
        let new_cursor = if ws_graphemes >= 2 {
            after_ws_skip
        } else if delim == WordDelim::WhiteSpace {
            self.move_one_word_left_pos(WordDelim::WhiteSpace)
        } else {
            self.fine_grained_word_left_pos_from(after_ws_skip, true)
        };

        assert!(new_cursor <= old_cursor_col);
        self.buf.drain(new_cursor..old_cursor_col);
        self.cursor_byte = Self::snap_to_grapheme_boundary_left(&self.buf, new_cursor);
    }

    pub fn delete_right_one_word(&mut self, delim: WordDelim) {
        self.push_snapshot(true);
        self.selection_byte = None;
        let end_cursor = right_one_word_end_pos_in(&self.buf, self.cursor_byte, delim);
        assert!(end_cursor >= self.cursor_byte);
        self.buf.drain(self.cursor_byte..end_cursor);
        self.cursor_byte = Self::snap_to_grapheme_boundary_left(&self.buf, self.cursor_byte);
    }

    pub fn replace_word_under_cursor(
        &mut self,
        new_word: &str,
        sub_string: &SubString,
    ) -> anyhow::Result<SubString> {
        let end = sub_string.start + sub_string.s.len();

        match self.buf.get(sub_string.start..end) {
            Some(s) if s == sub_string.s => {
                // Delete the word and position cursor at the start
                self.push_snapshot(false);
                self.buf.drain(sub_string.start..end);
                self.cursor_byte = sub_string.start;
                self.insert_str_no_snapshot(new_word);
                Ok(SubString {
                    s: new_word.to_string(),
                    start: sub_string.start,
                })
            }
            Some(s) => Err(anyhow::anyhow!(
                "Expected word '{}' at position {}, but found '{}'",
                sub_string.s,
                sub_string.start,
                s
            )),
            _ => Err(anyhow::anyhow!(
                "Expected word '{}' at position {}, but the range was out of bounds",
                sub_string.s,
                sub_string.start,
            )),
        }
    }

    pub fn is_cursor_on_s(&self, s: &str) -> Option<SubString> {
        if s.is_empty() {
            return None;
        }
        let cursor = self.cursor_byte;
        let mut start = 0;
        while let Some(pos) = self.buf[start..].find(s) {
            let actual_start = start + pos;
            let actual_end = actual_start + s.len();
            if actual_start <= cursor && cursor <= actual_end {
                return Some(SubString {
                    s: s.to_string(),
                    start: actual_start,
                });
            }
            let step = self.buf[actual_start..]
                .chars()
                .next()
                .map_or(1, char::len_utf8);
            start = actual_start + step;
        }
        None
    }

    pub fn replace_buffer(&mut self, new_buffer: &str) {
        self.push_snapshot(false);
        self.buf = new_buffer.to_string();
        self.cursor_byte = new_buffer.len();
        self.selection_byte = None;
    }

    pub fn delete_until_start_of_line(&mut self) {
        self.push_snapshot(true);
        self.selection_byte = None;
        let old_cursor = self.cursor_byte;
        self.move_start_of_line();
        self.buf.drain(self.cursor_byte..old_cursor);
        self.cursor_byte = Self::snap_to_grapheme_boundary_left(&self.buf, self.cursor_byte);
    }

    pub fn delete_until_end_of_line(&mut self) {
        self.push_snapshot(true);
        self.selection_byte = None;
        let old_cursor = self.cursor_byte;
        self.move_end_of_line();
        self.buf.drain(old_cursor..self.cursor_byte);
        self.cursor_byte = Self::snap_to_grapheme_boundary_left(&self.buf, old_cursor);
    }
}

/// `&str`-based core of [`TextBuffer::fine_grained_word_right_pos_from`].
fn fine_grained_word_right_pos_in(buf: &str, cursor_byte: usize, slash_aware: bool) -> usize {
    let end = buf.len();
    let class_fn: fn(&str) -> u8 = if slash_aware && TextBuffer::has_slash_in_word(buf, cursor_byte)
    {
        TextBuffer::less_strict_class_slash_only
    } else {
        TextBuffer::less_strict_class
    };
    let mut iter = buf
        .grapheme_indices(true)
        .skip_while(|(i, _)| *i < cursor_byte);
    match iter.next() {
        Some((_, first_g)) => {
            let class = class_fn(first_g);
            iter.find_map(|(i, g)| if class_fn(g) != class { Some(i) } else { None })
                .unwrap_or(end)
        }
        None => end,
    }
}

/// Computes the byte position of the end of one word to the right of
/// `start_cursor`, matching the reach of [`TextBuffer::delete_right_one_word`].
pub(crate) fn right_one_word_end_pos_in(buf: &str, start_cursor: usize, delim: WordDelim) -> usize {
    let end = buf.len();

    // First, find the position reached by skipping forward over any
    // contiguous run of whitespace immediately after `start_cursor`.
    let after_ws_skip = buf[start_cursor..]
        .grapheme_indices(true)
        .find(|(_, g)| !g.chars().next().is_some_and(char::is_whitespace))
        .map_or(end, |(i, _)| start_cursor + i);
    let ws_graphemes = buf[start_cursor..after_ws_skip].graphemes(true).count();

    // If there are 2+ contiguous whitespace graphemes, just consume the
    // whitespace. Otherwise (0 or 1 ws graphemes), also consume the next word
    // using the per-delim word-boundary logic.
    if ws_graphemes >= 2 {
        after_ws_skip
    } else if delim == WordDelim::WhiteSpace {
        buf.grapheme_indices(true)
            .skip_while(|(i, _)| *i <= start_cursor)
            .skip_while(|(_, g)| g.chars().next().is_some_and(|c| delim.is_word_boundary(c)))
            .find(|(_, g)| g.chars().next().is_some_and(|c| delim.is_word_boundary(c)))
            .map_or(end, |(i, _)| i)
    } else {
        fine_grained_word_right_pos_in(buf, after_ws_skip, true)
    }
}

#[cfg(test)]
mod test_editing_advanced {
    use super::*;

    #[test]
    fn right_one_word_end_pos_whitespace() {
        let s = "git status --short";
        assert_eq!(
            crate::right_one_word_end_pos(s, 3, WordDelim::WhiteSpace),
            "git status".len()
        );
        assert_eq!(
            crate::right_one_word_end_pos(s, "git status".len(), WordDelim::WhiteSpace),
            s.len()
        );
        assert_eq!(
            crate::right_one_word_end_pos("find café now", 4, WordDelim::WhiteSpace),
            "find café".len()
        );
        // Two or more contiguous whitespace chars are consumed alone.
        assert_eq!(
            crate::right_one_word_end_pos("a   b", 1, WordDelim::WhiteSpace),
            4
        );
    }

    #[test]
    fn right_one_word_end_pos_fine_grained() {
        let s = "git commit -m 'msg'";
        assert_eq!(
            crate::right_one_word_end_pos(s, "git commit".len(), WordDelim::FineGrained),
            "git commit -".len()
        );
        assert_eq!(
            crate::right_one_word_end_pos(s, "git commit -".len(), WordDelim::FineGrained),
            "git commit -m".len()
        );
        let p = "cd /etc/nginx/conf.d";
        assert_eq!(
            crate::right_one_word_end_pos(p, 2, WordDelim::FineGrained),
            "cd /".len()
        );
        assert_eq!(
            crate::right_one_word_end_pos(p, "cd /".len(), WordDelim::FineGrained),
            "cd /etc".len()
        );
    }

    #[test]
    fn delete_back() {
        let mut tb = TextBuffer::new("Hello, World!");
        tb.delete_left();
        assert_eq!(tb.buffer(), "Hello, World");
        tb.delete_left();
        assert_eq!(tb.buffer(), "Hello, Worl");
        tb.delete_left();
        assert_eq!(tb.buffer(), "Hello, Wor");
    }

    fn create_substring(buffer: &str, word: &str) -> SubString {
        let start = buffer.find(word).unwrap();
        SubString {
            s: word.to_string(),
            start,
        }
    }

    #[test]
    fn replace_word_under_cursor_at_start_of_line() {
        // Cursor at position 0 (start of line) with non-ASCII word
        let mut tb = TextBuffer::new("café option 日本語 🎯");
        tb.move_to_start(); // Cursor at position 0, at start of "café"
        tb.replace_word_under_cursor("coffee", &create_substring(tb.buffer(), "café"))
            .unwrap();
        assert_eq!(tb.buffer(), "coffee option 日本語 🎯");
        assert_eq!(tb.cursor_byte, "coffee".len());
    }

    #[test]
    fn replace_word_under_cursor_in_middle_of_word() {
        // Cursor in the middle of a word with Cyrillic characters
        let mut tb = TextBuffer::new("git файл --message 'привет' 🚀");
        tb.move_to_start();
        for _ in 0..6 {
            tb.move_right();
        } // Position at "git фа|йл" (middle of "файл")
        tb.replace_word_under_cursor("file", &create_substring(tb.buffer(), "файл"))
            .unwrap();
        assert_eq!(tb.buffer(), "git file --message 'привет' 🚀");
        assert_eq!(tb.cursor_byte, "git file".len());
    }

    #[test]
    fn replace_word_under_cursor_at_end_of_line() {
        // Cursor at the end of line on an emoji word
        let mut tb = TextBuffer::new("hello world 🎉🎊🎈");
        // Cursor is already at the end, on the emoji sequence
        tb.replace_word_under_cursor("celebration", &create_substring(tb.buffer(), "🎉🎊🎈"))
            .unwrap();
        assert_eq!(tb.buffer(), "hello world celebration");
        assert_eq!(tb.cursor_byte, "hello world celebration".len());
    }

    #[test]
    fn replace_word_under_cursor_accented_at_word_end() {
        // Cursor at the end of a word with heavy accents
        let mut tb = TextBuffer::new("find naïve résumé café 📄");
        tb.move_to_start();
        for _ in 0..10 {
            tb.move_right();
        } // Position at "find naïve| résumé" (end of "naïve")
        tb.replace_word_under_cursor("simple", &create_substring(tb.buffer(), "naïve"))
            .unwrap();
        assert_eq!(tb.buffer(), "find simple résumé café 📄");
        assert_eq!(tb.cursor_byte, "find simple".len());
    }

    #[test]
    #[should_panic(expected = "range was out of bounds")]
    fn replace_word_under_cursor_out_of_bounds() {
        // Cursor at the end of a word with heavy accents
        let mut tb = TextBuffer::new("find naïve résumé café 📄");
        tb.move_to_start();
        tb.replace_word_under_cursor(
            "test",
            &SubString {
                s: "nonexistent".to_string(),
                start: 100,
            },
        )
        .unwrap();
    }

    #[test]
    #[should_panic(expected = "Expected word 'wrong_word' at position 0, but found 'hello worl'")]
    fn replace_word_under_cursor_wrong_word() {
        // Cursor at the end of a word with heavy accents
        let mut tb = TextBuffer::new("hello world");
        tb.move_to_start();
        tb.replace_word_under_cursor(
            "test",
            &SubString {
                s: "wrong_word".to_string(),
                start: 0,
            },
        )
        .unwrap();
    }

    #[test]
    fn delete_one_word_left() {
        let mut tb = TextBuffer::new("cargo test abc::def::ghi   /etc/asd");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   ");
        // Two or more contiguous trailing whitespace chars are deleted alone,
        // without consuming the previous word.
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi");
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "cargo test ");
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "cargo ");
    }

    #[test]
    fn delete_one_word_left_trailing_whitespace_cases() {
        // Single trailing whitespace: delete the whitespace AND the previous word.
        let mut tb = TextBuffer::new("foo ");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "");

        // Two trailing whitespace chars: delete just the whitespace.
        let mut tb = TextBuffer::new("foo  ");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "foo");

        // Many trailing whitespace chars: delete just the whitespace.
        let mut tb = TextBuffer::new("foo           ");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "foo");

        // No trailing whitespace: delete the word.
        let mut tb = TextBuffer::new("foo");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "");
    }

    #[test]
    fn delete_one_word_left_less_strict() {
        let mut tb = TextBuffer::new("cargo test abc::def::ghi   /etc/asd");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   /etc/");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   /etc");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   /");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi   ");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def::ghi");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def::");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::def");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc::");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "cargo test abc");
    }

    #[test]
    fn delete_one_word_left_less_strict_single_space_also_deletes_word_part() {
        let mut tb = TextBuffer::new("foo bar");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "foo ");

        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "");
    }

    #[test]
    fn delete_one_word_right() {
        let mut tb = TextBuffer::new("cargo test abc::def::ghi   /etc/asd");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), " test abc::def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), " abc::def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "   /etc/asd");
        // Three or more contiguous leading whitespace chars are deleted alone,
        // without consuming the next word.
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "/etc/asd");
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "");
    }

    #[test]
    fn delete_one_word_right_leading_whitespace_cases() {
        // Single leading whitespace: delete the whitespace AND the next word.
        let mut tb = TextBuffer::new(" foo");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "");

        // Two leading whitespace chars: delete just the whitespace.
        let mut tb = TextBuffer::new("  foo");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "foo");

        // Many leading whitespace chars: delete just the whitespace.
        let mut tb = TextBuffer::new("                foo");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "foo");

        // No leading whitespace: delete the word.
        let mut tb = TextBuffer::new("foo");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert_eq!(tb.buffer(), "");
    }

    #[test]
    fn delete_one_word_right_less_strict() {
        let mut tb = TextBuffer::new("cargo test abc::def::ghi   /etc/asd");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), " test abc::def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), " abc::def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "::def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "def::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "::ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "ghi   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "   /etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "/etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "etc/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "/asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "asd");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "");
    }

    #[test]
    fn delete_one_word_right_less_strict_single_space_also_deletes_word_part() {
        let mut tb = TextBuffer::new("foo bar");
        tb.cursor_byte = "foo".len();
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "foo");
    }

    #[test]
    fn delete_one_word_left_less_strict_path() {
        // When the word to the left contains slashes, only / and \ are treated as
        // punctuation boundaries, so filename components with dots are not split.
        let mut tb = TextBuffer::new("echo ./foo_bar/baz.jeb");
        tb.move_end_of_line();
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo ./foo_bar/");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo ./foo_bar");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo ./");
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo .");
        // No more slashes in the remaining word; full punctuation mode resumes.
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo ");
    }

    #[test]
    fn delete_one_word_right_less_strict_path() {
        // Symmetric: forward deletion is also slash-aware.
        let mut tb = TextBuffer::new("echo ./foo_bar/baz.jeb");
        tb.move_start_of_line();
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), " ./foo_bar/baz.jeb");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "/foo_bar/baz.jeb");
        // After consuming the single leading space and '.', the remaining word
        // starts with '/' so slash-only mode applies.
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "foo_bar/baz.jeb");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "/baz.jeb");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "baz.jeb");
        // No more slashes; full punctuation mode.
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), ".jeb");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "jeb");
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "");
    }

    #[test]
    fn delete_one_word_left_slash_aware_from_right() {
        // When cursor is after a dotted path prefix but the slash is only to the
        // right of the cursor, slash-only mode should still apply so the whole
        // filename component is deleted as one unit.
        let mut tb = TextBuffer::new("echo baz.jeb/foo_bar");
        tb.cursor_byte = "echo baz.jeb".len();
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo /foo_bar");
    }

    #[test]
    fn delete_one_word_right_slash_aware_from_left() {
        // When cursor is right after the last slash in a path, the slash to the
        // left of the cursor should trigger slash-only mode so the dotted filename
        // component is deleted as one unit rather than being split at the dot.
        let mut tb = TextBuffer::new("echo /foo_bar/baz.jeb");
        tb.cursor_byte = "echo /foo_bar/".len();
        tb.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo /foo_bar/");
    }

    #[test]
    fn delete_word_backslash_path_bidirectional() {
        // Same behaviour with backslash path separators.
        let mut tb = TextBuffer::new("echo baz.txt\\foo\\bar");
        tb.cursor_byte = "echo baz.txt".len();
        tb.delete_one_word_left(WordDelim::FineGrained);
        assert_eq!(tb.buffer(), "echo \\foo\\bar");

        let mut tb2 = TextBuffer::new("echo \\foo\\baz.txt");
        tb2.cursor_byte = "echo \\foo\\".len();
        tb2.delete_right_one_word(WordDelim::FineGrained);
        assert_eq!(tb2.buffer(), "echo \\foo\\");
    }

    #[test]
    fn delete_until_end_of_line_multiline() {
        let mut tb = TextBuffer::new("hello\nworld\nfoo");
        tb.cursor_byte = 2; // Cursor after 'he|llo\nworld\nfoo'
        tb.delete_until_end_of_line();
        assert_eq!(tb.buffer(), "he\nworld\nfoo");
        // Move to next line and test again
        tb.cursor_byte = 3; // At start of 'world'
        tb.delete_until_end_of_line();
        assert_eq!(tb.buffer(), "he\n\nfoo");
    }

    #[test]
    fn delete_until_start_of_line_multiline() {
        let mut tb = TextBuffer::new("abc\ndef\nghi");
        tb.cursor_byte = 5;
        tb.delete_until_start_of_line();
        assert_eq!(tb.buffer(), "abc\nef\nghi");
        // Move to next line and test again
        tb.move_to_end();
        tb.delete_until_start_of_line();
        assert_eq!(tb.buffer(), "abc\nef\n");
    }

    #[test]
    fn test_is_cursor_on_s() {
        // Cursor at the end: "hello world|" (index 11)
        let tb = TextBuffer::new_with_cursor("hello world█");
        let sub = tb.is_cursor_on_s("world").unwrap();
        assert_eq!(sub.s, "world");
        assert_eq!(sub.start, 6);

        // Cursor inside: "hello wo|rld" (index 8)
        let tb = TextBuffer::new_with_cursor("hello wo█rld");
        let sub = tb.is_cursor_on_s("world").unwrap();
        assert_eq!(sub.s, "world");
        assert_eq!(sub.start, 6);

        // Cursor at start of word: "hello |world" (index 6)
        let tb = TextBuffer::new_with_cursor("hello █world");
        let sub = tb.is_cursor_on_s("world").unwrap();
        assert_eq!(sub.s, "world");
        assert_eq!(sub.start, 6);

        // Cursor not touching/on it: "|hello world" (index 0)
        let tb = TextBuffer::new_with_cursor("█hello world");
        assert!(tb.is_cursor_on_s("world").is_none());

        // Empty word
        let tb = TextBuffer::new_with_cursor("hello █world");
        assert!(tb.is_cursor_on_s("").is_none());

        // Multi-byte UTF-8 match occurring before the cursor must not panic when
        // advancing the search offset past the first match.
        let tb = TextBuffer::new_with_cursor("世界 foo 世界█");
        let sub = tb.is_cursor_on_s("世界").unwrap();
        assert_eq!(sub.s, "世界");
        assert_eq!(sub.start, "世界 foo ".len());
    }

    #[test]
    fn grapheme_boundaries_preserved_across_combining_marks_zwj_and_crlf() {
        // Inserting a base character before a lone combining mark fuses them into
        // one grapheme; cursor must snap to a grapheme boundary.
        let mut tb = TextBuffer::new("\u{0301}");
        tb.move_to_start();
        tb.insert_str("世界");
        assert_eq!(tb.buffer(), "世界\u{0301}");
        assert_eq!(tb.cursor_byte, tb.buffer().len());

        // Word movement across punctuation/whitespace with combining mark must not
        // split the grapheme cluster.
        let mut tb = TextBuffer::new("-\u{0301} abc");
        tb.move_to_start();
        tb.move_one_word_right_fine_grained();
        assert_eq!(tb.cursor_byte, "-\u{0301}".len());
        tb.move_one_word_left_fine_grained();
        assert_eq!(tb.cursor_byte, 0);

        // Moving to end/start of line across CRLF must not split `\r\n`.
        let mut tb = TextBuffer::new("hi\r\nbye");
        tb.move_to_start();
        tb.move_end_of_line();
        assert_eq!(tb.cursor_byte, 2);
        tb.move_to_end();
        tb.move_start_of_line();
        assert_eq!(tb.cursor_byte, 4);

        // Clicking/anchoring inside a ZWJ emoji must snap to a grapheme boundary.
        let mut tb = TextBuffer::new("a👩‍💻b");
        tb.try_move_cursor_to_byte_pos(5, false);
        assert_eq!(tb.cursor_byte, 1);
        tb.set_selection_anchor(8);
        assert_eq!(tb.selection_byte(), Some(1));

        // Deleting while a selection is active must not leave an out-of-bounds anchor.
        let mut tb = TextBuffer::new("hello world");
        tb.select_entire_buffer();
        tb.move_to_start();
        tb.set_selection_anchor(11);
        tb.delete_right_one_word(WordDelim::WhiteSpace);
        assert!(tb.selection_byte().is_none());
    }
}
