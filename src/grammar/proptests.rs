use super::command_acceptance::tests::{bash_n_accepts, find_system_bash};
use super::command_acceptance::will_bash_accept_buffer;
use super::dparser::{DParser, OpeningState, collect_tokens_include_whitespace};
use super::quoting::{QuoteType, dequoting_function_rust, find_quote_type, quoting_function_rust};
use crate::completions::context::get_completion_context;
use crate::shell::expand_shell_filename;
use proptest::prelude::*;

/// Generator biased toward Bash metacharacters, quoting, expansions, heredocs,
/// keywords, and multi-byte Unicode.
fn shell_fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("echo ".to_string()),
        Just("cat ".to_string()),
        Just(" ".to_string()),
        Just("\t".to_string()),
        Just("\n".to_string()),
        Just("'".to_string()),
        Just("\"".to_string()),
        Just("`".to_string()),
        Just("\\".to_string()),
        Just("$".to_string()),
        Just("$(".to_string()),
        Just("$((".to_string()),
        Just("${".to_string()),
        Just("$'".to_string()),
        Just("((".to_string()),
        Just("))".to_string()),
        Just("(".to_string()),
        Just(")".to_string()),
        Just("{".to_string()),
        Just("}".to_string()),
        Just("[".to_string()),
        Just("]".to_string()),
        Just("[[".to_string()),
        Just("]]".to_string()),
        Just("|".to_string()),
        Just("|&".to_string()),
        Just("||".to_string()),
        Just("&".to_string()),
        Just("&&".to_string()),
        Just("&>".to_string()),
        Just("&>>".to_string()),
        Just(";".to_string()),
        Just(";;".to_string()),
        Just(";&".to_string()),
        Just(";;&".to_string()),
        Just("<".to_string()),
        Just(">".to_string()),
        Just(">>".to_string()),
        Just("<<EOF".to_string()),
        Just("<<'EOF'".to_string()),
        Just("<<-EOF".to_string()),
        Just("<<<".to_string()),
        Just("<&".to_string()),
        Just(">&".to_string()),
        Just("<>".to_string()),
        Just(">|".to_string()),
        Just("<(".to_string()),
        Just(">(".to_string()),
        Just("@(".to_string()),
        Just("!(".to_string()),
        Just("#comment".to_string()),
        Just("=".to_string()),
        Just("if ".to_string()),
        Just("then ".to_string()),
        Just("else ".to_string()),
        Just("fi ".to_string()),
        Just("for ".to_string()),
        Just("select ".to_string()),
        Just("while ".to_string()),
        Just("until ".to_string()),
        Just("do ".to_string()),
        Just("done ".to_string()),
        Just("case ".to_string()),
        Just("in ".to_string()),
        Just("esac ".to_string()),
        Just("function ".to_string()),
        Just("EOF".to_string()),
        Just("foo".to_string()),
        Just("café".to_string()),
        Just("日本語".to_string()),
        Just("👩‍💻".to_string()),
    ]
}

fn shell_like_input() -> impl Strategy<Value = String> {
    prop::collection::vec(shell_fragment(), 0..24).prop_map(|v| v.concat())
}

/// Arbitrary non-empty filenames (no NUL bytes, as Unix filenames cannot contain `\0`).
fn filename_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just("a".to_string()),
            Just(" ".to_string()),
            Just("\t".to_string()),
            Just("\n".to_string()),
            Just("'".to_string()),
            Just("\"".to_string()),
            Just("`".to_string()),
            Just("\\".to_string()),
            Just("$".to_string()),
            Just("$HOME".to_string()),
            Just("$(echo x)".to_string()),
            Just("!".to_string()),
            Just("*".to_string()),
            Just("?".to_string()),
            Just("[a]".to_string()),
            Just(";".to_string()),
            Just("&".to_string()),
            Just("|".to_string()),
            Just("(".to_string()),
            Just(")".to_string()),
            Just("{".to_string()),
            Just("}".to_string()),
            Just("<".to_string()),
            Just(">".to_string()),
            Just("#".to_string()),
            Just("~".to_string()),
            Just("=".to_string()),
            Just("café".to_string()),
            Just("文件".to_string()),
            Just("👩‍💻".to_string()),
            "[a-zA-Z0-9._-]{1,4}",
        ],
        1..12,
    )
    .prop_map(|parts| parts.concat())
}

fn quote_type_strategy() -> impl Strategy<Value = QuoteType> {
    prop_oneof![
        Just(QuoteType::SingleQuote),
        Just(QuoteType::DoubleQuote),
        Just(QuoteType::Backslash),
    ]
}

/// Structured valid Bash command generator for differential testing against `bash -n`.
fn structured_bash_command() -> impl Strategy<Value = String> {
    let simple = prop_oneof![
        Just("echo hi".to_string()),
        Just("echo 'hello world'".to_string()),
        Just("echo \"hello $USER\"".to_string()),
        Just("true".to_string()),
        Just("false".to_string()),
        Just("arr=(1 2 3)".to_string()),
        Just("local arr=(a b)".to_string()),
        Just("echo $(echo ok)".to_string()),
        Just("echo $((1 + 2))".to_string()),
        Just("(( 1 + 2 ))".to_string()),
        Just("[[ ( a == b ) ]]".to_string()),
        Just("echo `echo ok`".to_string()),
        Just("echo ${HOME:-/tmp}".to_string()),
        Just("cat <(echo hi)".to_string()),
        Just("echo a &> /dev/null".to_string()),
        Just("echo a &>> /dev/null".to_string()),
    ];

    let compound = simple.clone().prop_flat_map(|body| {
        prop_oneof![
            Just(body.clone()),
            Just(format!("({body})")),
            Just(format!("{{ {body}; }}")),
            Just(format!("if true; then {body}; fi")),
            Just(format!("while false; do {body}; done")),
            Just(format!("until true; do {body}; done")),
            Just(format!("for i in 1 2; do {body}; done")),
            Just(format!("select i in 1 2; do {body}; done")),
            Just(format!(
                "case x in a) {body} ;; (b) echo 2 ;& *) echo 3 ;; esac"
            )),
            Just(format!("f() {{ {body}; }}")),
            Just(format!("f() ({body})")),
            Just(format!("time ({body})")),
            Just(format!("! ({body})")),
            Just(format!("cat <<EOF\n{body}\nEOF")),
        ]
    });

    (
        compound.clone(),
        prop_oneof![
            Just(" | "),
            Just(" |& "),
            Just(" && "),
            Just(" || "),
            Just("; "),
            Just("\n"),
        ],
        compound,
        any::<bool>(),
    )
        .prop_map(|(lhs, sep, rhs, combine)| {
            if combine {
                format!("{lhs}{sep}{rhs}")
            } else {
                lhs
            }
        })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn lexer_roundtrips_arbitrary_unicode_and_has_contiguous_byte_ranges(
        input in prop::collection::vec(
            prop_oneof![
                any::<char>().prop_filter("non-nul", |c| *c != '\0'),
                prop::char::range('\x01', '\x7f'),
            ],
            0..64,
        )
        .prop_map(|chars| chars.into_iter().collect::<String>()),
    ) {
        let tokens = collect_tokens_include_whitespace(&input);
        let mut reconstructed = String::with_capacity(input.len());
        let mut expected_byte = 0usize;

        for tok in &tokens {
            let range = tok.byte_range();
            prop_assert!(
                input.is_char_boundary(range.start) && input.is_char_boundary(range.end),
                "non-char-boundary range {:?} in {:?} for token {:?}",
                range,
                input,
                tok
            );
            prop_assert_eq!(
                range.start,
                expected_byte,
                "gap/overlap before token {:?} in {:?}",
                tok,
                input
            );
            prop_assert_eq!(
                &input[range.clone()],
                tok.value.as_str(),
                "token slice mismatch for {:?} in {:?}",
                tok,
                input
            );
            reconstructed.push_str(&tok.value);
            expected_byte = range.end;
        }

        prop_assert_eq!(expected_byte, input.len(), "tokens did not cover full input {:?}", input);
        prop_assert_eq!(reconstructed, input);
    }

    #[test]
    fn lexer_and_dparser_invariants_hold_on_shell_like_inputs(
        input in shell_like_input(),
    ) {
        // 1. Lexer round-trip + contiguous byte ranges.
        let tokens = collect_tokens_include_whitespace(&input);
        let reconstructed: String = tokens.iter().map(|t| t.token_value_str()).collect();
        prop_assert_eq!(&reconstructed, &input);

        let mut expected_byte = 0usize;
        for tok in &tokens {
            let range = tok.byte_range();
            prop_assert_eq!(range.start, expected_byte);
            prop_assert_eq!(&input[range.clone()], tok.value.as_str());
            expected_byte = range.end;
        }
        prop_assert_eq!(expected_byte, input.len());

        // 2. DParser full walk never panics and opening/closing annotations are symmetric.
        let annotated = DParser::parse_and_annotate(&input);
        for (idx, tok) in annotated.iter().enumerate() {
            if let Some(OpeningState::Matched(close_idx)) = tok.annotations.opening {
                prop_assert!(close_idx < annotated.len());
                let closing = annotated[close_idx].annotations.closing.as_ref();
                prop_assert!(
                    closing.is_some_and(|c| c.opening_idx == idx),
                    "asymmetric opening {} -> {} on {:?}",
                    idx,
                    close_idx,
                    input
                );
            }
            if let Some(closing) = &tok.annotations.closing {
                prop_assert!(closing.opening_idx < annotated.len());
                prop_assert_eq!(
                    annotated[closing.opening_idx].annotations.opening.as_ref(),
                    Some(&OpeningState::Matched(idx)),
                    "asymmetric closing {} -> {} on {:?}",
                    idx,
                    closing.opening_idx,
                    input
                );
            }
        }

        // 3. Command acceptance, cursor walking, completion context, and filename
        //    expansion never panic on arbitrary shell-like inputs.
        let _ = will_bash_accept_buffer(&input);
        let _ = expand_shell_filename(&input, |_| Some("/val".to_string()));
        let _ = find_quote_type(&input);
        let _ = dequoting_function_rust(&input);

        // Sample cursor positions at char boundaries across the input.
        let mut positions: Vec<usize> = input.char_indices().map(|(i, _)| i).collect();
        positions.push(input.len());
        let step = (positions.len() / 8).max(1);
        for &pos in positions.iter().step_by(step) {
            let mut p = DParser::from(&input);
            p.walk_to_cursor(pos);
            let _ = get_completion_context(&input, pos);
        }
    }

    #[test]
    fn quoting_and_dequoting_roundtrip_for_arbitrary_filenames(
        filename in filename_strategy(),
        qt in quote_type_strategy(),
    ) {
        let quoted = quoting_function_rust(&filename, qt, true, true);
        let unquoted = dequoting_function_rust(&quoted);
        prop_assert_eq!(
            unquoted,
            filename,
            "roundtrip failed for qt={:?}, quoted={:?}",
            qt,
            quoted
        );
    }

    #[test]
    fn quoting_roundtrip_matches_real_bash(
        filename in filename_strategy(),
        qt in quote_type_strategy(),
    ) {
        let Some(bash) = find_system_bash() else {
            return Ok(());
        };
        let quoted = quoting_function_rust(&filename, qt, true, true);
        // Disable histexpand (`set +H`, default in non-interactive bash -c) and
        // pass the quoted word to `printf '%s'` as a single shell argument.
        let script = format!("printf '%s' {quoted}");
        let output = std::process::Command::new(&bash)
            .args(["-c", &script])
            .output()
            .expect("failed to execute bash");
        prop_assert!(
            output.status.success(),
            "bash failed (status {:?}) for filename={:?}, qt={:?}, quoted={:?}, stderr={:?}",
            output.status,
            filename,
            qt,
            quoted,
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        prop_assert_eq!(
            stdout.as_ref(),
            filename.as_str(),
            "bash evaluation mismatch for filename={:?}, qt={:?}, quoted={:?}",
            filename,
            qt,
            quoted
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 80,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn differential_command_acceptance_random_commands_and_prefixes_vs_bash_n(
        cmd in structured_bash_command(),
        truncate_frac in 0u8..=100u8,
    ) {
        let Some(bash) = find_system_bash() else {
            return Ok(());
        };

        // Pick either the full command or a char-boundary prefix of it.
        let candidate = if truncate_frac >= 80 {
            cmd.as_str()
        } else {
            let target_byte = (cmd.len() * usize::from(truncate_frac)) / 100;
            let mut cut = target_byte.min(cmd.len());
            while cut > 0 && !cmd.is_char_boundary(cut) {
                cut -= 1;
            }
            &cmd[..cut]
        };

        // Skip trailing unescaped backslash because `bash -n` with appended `\n`
        // consumes `\\\n` as a line continuation into an empty second line.
        let trimmed = candidate.trim_end_matches([' ', '\t']);
        if trimmed.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1 {
            return Ok(());
        }

        if let Some(bash_accepts) = bash_n_accepts(&bash, candidate) {
            let flyline_accepts = will_bash_accept_buffer(candidate);
            prop_assert_eq!(
                flyline_accepts,
                bash_accepts,
                "disagreement with bash -n for candidate={:?} (from full={:?}): flyline={}, bash={}",
                candidate,
                cmd,
                flyline_accepts,
                bash_accepts
            );
        }
    }
}

trait TokenValueExt {
    fn token_value_str(&self) -> &str;
}

impl TokenValueExt for super::lexer::Token {
    fn token_value_str(&self) -> &str {
        &self.value
    }
}
