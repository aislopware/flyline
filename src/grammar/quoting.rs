// QuoteType can be in the middle of a word (i.e. backslash)
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub enum QuoteType {
    SingleQuote,
    DoubleQuote,
    #[default]
    Backslash,
}

impl QuoteType {
    pub fn from_char(c: char) -> Option<QuoteType> {
        match c {
            '\'' => Some(QuoteType::SingleQuote),
            '"' => Some(QuoteType::DoubleQuote),
            '\\' => Some(QuoteType::Backslash),
            _ => None,
        }
    }

    pub fn into_byte(self) -> u8 {
        match self {
            QuoteType::SingleQuote => b'\'',
            QuoteType::DoubleQuote => b'"',
            QuoteType::Backslash => b'\\',
        }
    }
}

pub const DOUBLE_QUOTE_SPECIAL_CHARS: &[char] = &['$', '`', '"', '\\'];
pub const BACKSLASH_SPECIAL_CHARS: &[char] = &[
    ' ', '\t', '\\', '"', '\'', '!', '$', '&', '(', ')', '*', ';', '<', '>', '?', '[', ']', '^',
    '`', '{', '|', '}', '#', '~',
];

/* Quote a filename using double quotes, single quotes, or backslashes
depending on the value of completion_quoting_style.  If we're
completing using backslashes, we need to quote some additional
characters (those that readline treats as word breaks), so we call
quote_word_break_chars on the result.  This returns newly-allocated
memory. */
// static char * bash_quote_filename (char *s, int rtype, char *qcp)
// TODO: handle edge cases that bash_quote_filename handles
pub fn quoting_function_rust(
    s: &str,
    quote_type: QuoteType,
    opening_quote: bool,
    closing_quote: bool,
) -> String {
    match quote_type {
        QuoteType::SingleQuote => {
            let mut quoted = s.replace('\'', "'\\''");
            if opening_quote {
                quoted = format!("'{}", quoted);
            }
            if closing_quote {
                quoted.push('\'');
            }
            quoted
        }
        QuoteType::DoubleQuote => {
            let mut escaped = String::with_capacity(s.len());
            for c in s.chars() {
                if DOUBLE_QUOTE_SPECIAL_CHARS.contains(&c) {
                    escaped.push('\\');
                    escaped.push(c);
                } else if c == '!' {
                    // In Bash, `"\!"` retains the literal backslash under `set +H`,
                    // while unescaped `"!"` triggers history expansion under `set -H`.
                    // Closing the double quote, backslash-escaping `!`, and reopening
                    // evaluates to literal `!` under both modes.
                    escaped.push_str("\"\\!\"");
                } else {
                    // Literal newlines inside double quotes stay intact in Bash
                    // (whereas `\<newline>` would be deleted as a line continuation).
                    escaped.push(c);
                }
            }

            let mut quoted = if opening_quote {
                format!("\"{}", escaped)
            } else {
                escaped
            };
            if closing_quote {
                quoted.push('"');
            }
            quoted
        }
        QuoteType::Backslash => {
            let mut quoted = String::with_capacity(s.len());
            for c in s.chars() {
                if c == '\n' {
                    // `\<newline>` is a Bash line continuation (drops the newline);
                    // single-quoting the newline preserves it literally, matching
                    // GNU Bash's `bash_quote_filename`.
                    quoted.push_str("'\n'");
                } else if c.is_whitespace() || BACKSLASH_SPECIAL_CHARS.contains(&c) {
                    quoted.push('\\');
                    quoted.push(c);
                } else {
                    quoted.push(c);
                }
            }
            quoted
        }
    }
}

/* Filename quoting for completion. */
/* A function to strip unquoted quote characters (single quotes, double
quotes, and backslashes).  It allows single quotes to appear
within double quotes, and vice versa.  It should be smarter. */
// static char *bash_dequote_filename (char *text, int quote_char)
pub fn dequoting_function_rust(s: &str) -> String {
    let mut res = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next_char) = chars.next()
                    && next_char != '\n'
                {
                    res.push(next_char);
                }
            }
            '\'' => {
                for next_char in chars.by_ref() {
                    if next_char == '\'' {
                        break;
                    }
                    res.push(next_char);
                }
            }
            '"' => {
                while let Some(next_char) = chars.next() {
                    if next_char == '"' {
                        break;
                    }
                    if next_char == '\\' {
                        match chars.next() {
                            Some('\n') => {}
                            Some(escaped) if DOUBLE_QUOTE_SPECIAL_CHARS.contains(&escaped) => {
                                res.push(escaped);
                            }
                            Some(other) => {
                                res.push('\\');
                                res.push(other);
                            }
                            None => res.push('\\'),
                        }
                    } else {
                        res.push(next_char);
                    }
                }
            }
            _ => res.push(c),
        }
    }
    res
}

// This function
//    returns the opening quote character if we found an unclosed quoted
//    substring, '\0' otherwise.  FP, if non-null, is set to a value saying
//    which (shell-like) quote characters we found (single quote, double
//    quote, or backslash) anywhere in the string.  DP, if non-null, is set to
//    the value of the delimiter character that caused a word break. */
// It sets fp to  a bitfield  but no one ever reads that bitfield so we can ignore it for now
// char _rl_find_completion_word (int *fp, int *dp)

pub fn find_quote_type(s: &str) -> Option<QuoteType> {
    if s.starts_with('\"') {
        return Some(QuoteType::DoubleQuote);
    } else if s.starts_with('\'') {
        return Some(QuoteType::SingleQuote);
    } else {
        // Check for odd number of consecutive backslashes
        let mut backslash_count = 0;
        let mut max_consecutive_backslashes = 0;

        for c in s.chars() {
            if c == '\\' {
                backslash_count += 1;
            } else if backslash_count > 0 {
                max_consecutive_backslashes = max_consecutive_backslashes.max(backslash_count);
                backslash_count = 0;
            }
        }
        // Handle case where string ends with backslashes
        if backslash_count > 0 {
            max_consecutive_backslashes = max_consecutive_backslashes.max(backslash_count);
        }

        if max_consecutive_backslashes > 0 && max_consecutive_backslashes % 2 == 1 {
            return Some(QuoteType::Backslash);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_function() {
        assert_eq!(
            quoting_function_rust(r#"qwe asd"#, QuoteType::Backslash, true, true),
            r#"qwe\ asd"#
        );
        assert_eq!(
            quoting_function_rust(r#"qwe asd"#, QuoteType::DoubleQuote, true, true),
            r#""qwe asd""#
        );
        assert_eq!(
            quoting_function_rust(r#"qwe asd"#, QuoteType::SingleQuote, true, true),
            r#"'qwe asd'"#
        );
    }

    #[test]
    fn test_quote_function_harder() {
        assert_eq!(
            quoting_function_rust(r#"qwe"asdf"#, QuoteType::Backslash, true, true),
            r#"qwe\"asdf"#
        );
        assert_eq!(
            quoting_function_rust(r#"qwe"asdf"#, QuoteType::DoubleQuote, true, true),
            r#""qwe\"asdf""#
        );
    }

    #[test]
    fn test_quote_function_backslash_special_chars() {
        for &c in BACKSLASH_SPECIAL_CHARS {
            let input = format!("a{}b", c);
            let expected = format!("a\\{}b", c);
            assert_eq!(
                quoting_function_rust(&input, QuoteType::Backslash, true, true),
                expected
            );
        }
    }

    #[test]
    fn test_quote_function_double_quote_special_chars() {
        for &c in DOUBLE_QUOTE_SPECIAL_CHARS {
            let input = format!("a{}b", c);
            let expected_inner = format!("a\\{}b", c);
            let expected = format!("\"{}\"", expected_inner);
            assert_eq!(
                quoting_function_rust(&input, QuoteType::DoubleQuote, true, true),
                expected
            );
        }
    }

    #[test]
    fn test_dequoting_function() {
        assert_eq!(dequoting_function_rust(r#"qwe\ asd"#), r#"qwe asd"#);
        assert_eq!(dequoting_function_rust(r#""qwe asd""#), r#"qwe asd"#);
        assert_eq!(dequoting_function_rust(r#"'qwe asd'"#), r#"qwe asd"#);
        assert_eq!(dequoting_function_rust(r#"abc"#), r#"abc"#);
    }

    #[test]
    fn test_dequoting_function_harder() {
        assert_eq!(dequoting_function_rust(r#"qwe\"asdf"#), r#"qwe"asdf"#);
        assert_eq!(dequoting_function_rust(r#""qwe\"asdf""#), r#"qwe"asdf"#);
        assert_eq!(dequoting_function_rust(r#""""#), r#""#);
        // Inside double quotes, backslash only escapes $, `, ", \ (and line-continuation \n).
        assert_eq!(dequoting_function_rust(r#""a\nb""#), r#"a\nb"#);
        assert_eq!(dequoting_function_rust("\"a\\\nb\""), "ab");
    }

    #[test]
    fn test_quote_newline_bang_hash_and_tilde_match_bash() {
        // Double quotes: literal newline (not `\<newline>` line continuation),
        // and `"\!"` escape for `!` so it works under both `set -H` and `set +H`.
        assert_eq!(
            quoting_function_rust("a\nb", QuoteType::DoubleQuote, true, true),
            "\"a\nb\""
        );
        assert_eq!(
            quoting_function_rust("a!b", QuoteType::DoubleQuote, true, true),
            "\"a\"\\!\"b\""
        );
        assert_eq!(
            dequoting_function_rust(&quoting_function_rust(
                "a!b\nc",
                QuoteType::DoubleQuote,
                true,
                true
            )),
            "a!b\nc"
        );

        // Backslash quoting: single-quote newline (not `\<newline>` line continuation),
        // and escape `#` and `~`.
        assert_eq!(
            quoting_function_rust("a\nb", QuoteType::Backslash, true, true),
            "a'\n'b"
        );
        assert_eq!(
            quoting_function_rust("#foo", QuoteType::Backslash, true, true),
            "\\#foo"
        );
        assert_eq!(
            quoting_function_rust("~foo", QuoteType::Backslash, true, true),
            "\\~foo"
        );
        assert_eq!(
            dequoting_function_rust(&quoting_function_rust(
                "#a~b\nc",
                QuoteType::Backslash,
                true,
                true
            )),
            "#a~b\nc"
        );
    }

    #[test]
    fn test_find_quotes() {
        assert_eq!(
            find_quote_type(r#""qwe asdf"#),
            Some(QuoteType::DoubleQuote)
        );
        assert_eq!(
            find_quote_type(r#"'qwe asdf"#),
            Some(QuoteType::SingleQuote)
        );
        assert_eq!(find_quote_type(r#"qwe\ asdf"#), Some(QuoteType::Backslash));
        assert_eq!(find_quote_type(r#"qwe asdf"#), None);
        assert_eq!(find_quote_type(r#"qwe\\asdf"#), None);
    }
}
