use crate::{TextBuffer, WordDelim};
use proptest::prelude::*;
use unicode_segmentation::UnicodeSegmentation;

fn is_grapheme_boundary(s: &str, byte_pos: usize) -> bool {
    if byte_pos == 0 || byte_pos == s.len() {
        return true;
    }
    if byte_pos > s.len() || !s.is_char_boundary(byte_pos) {
        return false;
    }
    s.grapheme_indices(true).any(|(idx, _)| idx == byte_pos)
}

fn assert_invariants(tb: &TextBuffer, op_desc: &str) {
    let buf = tb.buffer();
    let cursor = tb.cursor_byte_pos();
    assert!(
        cursor <= buf.len(),
        "cursor {cursor} > buf.len() {} after {op_desc} on {buf:?}",
        buf.len()
    );
    assert!(
        buf.is_char_boundary(cursor),
        "cursor {cursor} is not on a UTF-8 char boundary after {op_desc} on {buf:?}"
    );
    assert!(
        is_grapheme_boundary(buf, cursor),
        "cursor {cursor} is not on a grapheme boundary after {op_desc} on {buf:?}"
    );

    if let Some(sel) = tb.selection_byte() {
        assert!(
            sel <= buf.len(),
            "selection anchor {sel} > buf.len() {} after {op_desc} on {buf:?}",
            buf.len()
        );
        assert!(
            buf.is_char_boundary(sel),
            "selection anchor {sel} is not on a UTF-8 char boundary after {op_desc} on {buf:?}"
        );
        assert!(
            is_grapheme_boundary(buf, sel),
            "selection anchor {sel} is not on a grapheme boundary after {op_desc} on {buf:?}"
        );
    }

    // Accessors must never panic.
    let _ = tb.cursor_2d_position();
    let _ = tb.selection_range();
    let _ = tb.selected_text();
}

#[derive(Debug, Clone)]
enum BufferOp {
    InsertChar(char),
    InsertStr(String),
    InsertNewline,
    DeleteLeft,
    DeleteRight,
    DeleteWordLeft(WordDelim),
    DeleteWordRight(WordDelim),
    DeleteUntilStartOfLine,
    DeleteUntilEndOfLine,
    MoveLeft,
    MoveRight,
    MoveLeftSelection,
    MoveRightSelection,
    MoveWordLeft(WordDelim),
    MoveWordRight(WordDelim),
    MoveWordLeftFineGrained,
    MoveWordRightFineGrained,
    MoveWordLeftStrict,
    MoveWordRightStrict,
    MoveRightWordExtendSelection,
    MoveLeftWordExtendSelection,
    MoveStartOfLine,
    MoveEndOfLine,
    MoveToStart,
    MoveToEnd,
    MoveLineUp,
    MoveLineDown,
    TryMoveToBytePos(usize, bool),
    SetSelectionAnchor(usize),
    SelectWordMouse,
    SelectLineMouse,
    SelectAll,
    ClearSelection,
    DeleteSelection,
    SurroundSelection(char, char),
    ReplaceBuffer(String),
    IsCursorOnS(String),
    Undo,
    Redo,
}

fn rich_text_piece() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("a".to_string()),
        Just(" ".to_string()),
        Just("  ".to_string()),
        Just("\n".to_string()),
        Just("\n\n".to_string()),
        Just("\r\n".to_string()),
        Just("/".to_string()),
        Just("-".to_string()),
        Just(".".to_string()),
        Just("foo/bar".to_string()),
        Just("café".to_string()),
        Just("e\u{0301}".to_string()),
        Just("-\u{0301}".to_string()),
        Just(" \u{0301}".to_string()),
        Just("世界".to_string()),
        Just("👩‍💻".to_string()),
        Just("👨‍👩‍👧‍👦".to_string()),
        Just("🇺🇸".to_string()),
        Just("👋🏻".to_string()),
        Just("\u{0301}".to_string()),
        "[a-z0-9 _/\\-\\.]{0,6}",
    ]
}

fn rich_string() -> impl Strategy<Value = String> {
    prop::collection::vec(rich_text_piece(), 0..8).prop_map(|parts| parts.concat())
}

fn rich_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('a'),
        Just(' '),
        Just('\n'),
        Just('\r'),
        Just('/'),
        Just('-'),
        Just('é'),
        Just('\u{0301}'),
        Just('世'),
        Just('👩'),
        Just('\u{200d}'),
        Just('💻'),
        Just('🇺'),
        Just('🇸'),
    ]
}

fn word_delim() -> impl Strategy<Value = WordDelim> {
    prop_oneof![Just(WordDelim::WhiteSpace), Just(WordDelim::FineGrained)]
}

fn buffer_op() -> impl Strategy<Value = BufferOp> {
    prop_oneof![
        rich_char().prop_map(BufferOp::InsertChar),
        rich_text_piece().prop_map(BufferOp::InsertStr),
        Just(BufferOp::InsertNewline),
        Just(BufferOp::DeleteLeft),
        Just(BufferOp::DeleteRight),
        word_delim().prop_map(BufferOp::DeleteWordLeft),
        word_delim().prop_map(BufferOp::DeleteWordRight),
        Just(BufferOp::DeleteUntilStartOfLine),
        Just(BufferOp::DeleteUntilEndOfLine),
        Just(BufferOp::MoveLeft),
        Just(BufferOp::MoveRight),
        Just(BufferOp::MoveLeftSelection),
        Just(BufferOp::MoveRightSelection),
        word_delim().prop_map(BufferOp::MoveWordLeft),
        word_delim().prop_map(BufferOp::MoveWordRight),
        Just(BufferOp::MoveWordLeftFineGrained),
        Just(BufferOp::MoveWordRightFineGrained),
        Just(BufferOp::MoveWordLeftStrict),
        Just(BufferOp::MoveWordRightStrict),
        Just(BufferOp::MoveRightWordExtendSelection),
        Just(BufferOp::MoveLeftWordExtendSelection),
        Just(BufferOp::MoveStartOfLine),
        Just(BufferOp::MoveEndOfLine),
        Just(BufferOp::MoveToStart),
        Just(BufferOp::MoveToEnd),
        Just(BufferOp::MoveLineUp),
        Just(BufferOp::MoveLineDown),
        (0usize..64, any::<bool>()).prop_map(|(p, past)| BufferOp::TryMoveToBytePos(p, past)),
        (0usize..64).prop_map(BufferOp::SetSelectionAnchor),
        Just(BufferOp::SelectWordMouse),
        Just(BufferOp::SelectLineMouse),
        Just(BufferOp::SelectAll),
        Just(BufferOp::ClearSelection),
        Just(BufferOp::DeleteSelection),
        Just(BufferOp::SurroundSelection('(', ')')),
        rich_string().prop_map(BufferOp::ReplaceBuffer),
        rich_text_piece().prop_map(BufferOp::IsCursorOnS),
        Just(BufferOp::Undo),
        Just(BufferOp::Redo),
    ]
}

fn apply_op(tb: &mut TextBuffer, op: &BufferOp) {
    match op {
        BufferOp::InsertChar(c) => tb.insert_char(*c),
        BufferOp::InsertStr(s) => tb.insert_str(s),
        BufferOp::InsertNewline => tb.insert_newline(),
        BufferOp::DeleteLeft => tb.delete_left(),
        BufferOp::DeleteRight => tb.delete_right(),
        BufferOp::DeleteWordLeft(d) => tb.delete_one_word_left(*d),
        BufferOp::DeleteWordRight(d) => tb.delete_right_one_word(*d),
        BufferOp::DeleteUntilStartOfLine => tb.delete_until_start_of_line(),
        BufferOp::DeleteUntilEndOfLine => tb.delete_until_end_of_line(),
        BufferOp::MoveLeft => tb.move_left(),
        BufferOp::MoveRight => tb.move_right(),
        BufferOp::MoveLeftSelection => tb.move_left_selection(),
        BufferOp::MoveRightSelection => tb.move_right_selection(),
        BufferOp::MoveWordLeft(d) => tb.move_one_word_left(*d),
        BufferOp::MoveWordRight(d) => tb.move_one_word_right(*d),
        BufferOp::MoveWordLeftFineGrained => tb.move_one_word_left_fine_grained(),
        BufferOp::MoveWordRightFineGrained => tb.move_one_word_right_fine_grained(),
        BufferOp::MoveWordLeftStrict => tb.move_one_word_left_strict(),
        BufferOp::MoveWordRightStrict => tb.move_one_word_right_strict(),
        BufferOp::MoveRightWordExtendSelection => {
            tb.move_right_one_word_whitespace_extend_selection();
        }
        BufferOp::MoveLeftWordExtendSelection => {
            tb.move_left_one_word_whitespace_extend_selection();
        }
        BufferOp::MoveStartOfLine => tb.move_start_of_line(),
        BufferOp::MoveEndOfLine => tb.move_end_of_line(),
        BufferOp::MoveToStart => tb.move_to_start(),
        BufferOp::MoveToEnd => tb.move_to_end(),
        BufferOp::MoveLineUp => tb.move_line_up(),
        BufferOp::MoveLineDown => tb.move_line_down(),
        BufferOp::TryMoveToBytePos(pos, past) => tb.try_move_cursor_to_byte_pos(*pos, *past),
        BufferOp::SetSelectionAnchor(pos) => tb.set_selection_anchor(*pos),
        BufferOp::SelectWordMouse => {
            let _ = tb.select_word_using_mouse();
        }
        BufferOp::SelectLineMouse => {
            let _ = tb.select_line_using_mouse();
        }
        BufferOp::SelectAll => tb.select_entire_buffer(),
        BufferOp::ClearSelection => tb.clear_selection(),
        BufferOp::DeleteSelection => {
            let _ = tb.delete_selection();
        }
        BufferOp::SurroundSelection(open, close) => {
            let _ = tb.surround_selection(*open, *close);
        }
        BufferOp::ReplaceBuffer(s) => tb.replace_buffer(s),
        BufferOp::IsCursorOnS(s) => {
            let _ = tb.is_cursor_on_s(s);
        }
        BufferOp::Undo => tb.undo(),
        BufferOp::Redo => tb.redo(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn buffer_ops_never_panic_and_preserve_grapheme_boundaries(
        initial in rich_string(),
        ops in prop::collection::vec(buffer_op(), 1..40),
    ) {
        let mut tb = TextBuffer::new(&initial);
        assert_invariants(&tb, "new");

        for (step, op) in ops.iter().enumerate() {
            apply_op(&mut tb, op);
            assert_invariants(&tb, &format!("step {step}: {op:?}"));
        }
    }

    #[test]
    fn undo_redo_restores_buffer_and_cursor_on_mutating_edits(
        initial in rich_string(),
        moves in prop::collection::vec(
            prop_oneof![
                Just(BufferOp::MoveLeft),
                Just(BufferOp::MoveRight),
                Just(BufferOp::MoveStartOfLine),
                Just(BufferOp::MoveEndOfLine),
                Just(BufferOp::MoveToStart),
                Just(BufferOp::MoveToEnd),
            ],
            0..10,
        ),
        insertion in rich_text_piece(),
    ) {
        let mut tb = TextBuffer::new(&initial);
        for op in &moves {
            apply_op(&mut tb, op);
        }

        let before_buf = tb.buffer().to_string();
        let before_cursor = tb.cursor_byte_pos();

        tb.insert_str(&insertion);
        assert_invariants(&tb, "insert_str");

        let after_buf = tb.buffer().to_string();
        let after_cursor = tb.cursor_byte_pos();

        if after_buf != before_buf {
            tb.undo();
            assert_invariants(&tb, "undo");
            prop_assert_eq!(tb.buffer(), before_buf.as_str());
            prop_assert_eq!(tb.cursor_byte_pos(), before_cursor);

            tb.redo();
            assert_invariants(&tb, "redo");
            prop_assert_eq!(tb.buffer(), after_buf.as_str());
            prop_assert_eq!(tb.cursor_byte_pos(), after_cursor);
        }
    }
}
