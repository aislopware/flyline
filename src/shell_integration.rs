use crate::content::RelativePosition;
use crate::shell;
use crate::term_info;

/// Percent-encode `s` for a `file://` URI (RFC 3986), keeping the unreserved
/// characters and those in `keep`.
///
/// Terminals parse OSC 7 as a URI: a raw space, `#`, `?` or `%` in a directory name
/// gives them the wrong path, and a raw control character would end the sequence.
fn percent_encode_uri_part(s: &str, keep: &[u8]) -> String {
    let mut out = String::with_capacity(s.len());
    for &byte in s.as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) || keep.contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// <https://code.visualstudio.com/docs/terminal/shell-integration#_supported-escape-sequences>
/// <https://sw.kovidgoyal.net/kitty/shell-integration/>
/// <https://ghostty.org/docs/features/shell-integration#troubleshooting>
/// Ghostty right click -> Terminal inspector -> Terminal IO -> Filter for osc
/// And you click on the event to see more details.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscapeCodes {
    // OSC 7
    CurrentDirectory {
        host: String,
        path: String,
    },
    KittyCurrentDirectory {
        host: String,
        path: String,
    },

    // OSC 2
    WindowTitle {
        title: String,
    },

    // OSC 133 (FinalTerm)
    PromptStart {
        rel_pos: RelativePosition,
    },
    PromptEnd {
        rel_pos: RelativePosition,
    },
    PreExecution {
        commandline: Option<String>,
    },
    ExecutionFinished {
        exit_code: Option<i32>,
    },

    // OSC 633 (VS Code)
    VscProperties {
        cwd: String,
        has_rich_command_detection: bool,
    },
    VscPromptStart {
        rel_pos: RelativePosition,
    },
    VscPromptEnd {
        rel_pos: RelativePosition,
    },
    VscPreExecution, // Vs Code has another escape code for sending the command
    VscExecutionFinished {
        exit_code: Option<i32>,
    },
    VscCommandLine {
        commandline: String,
        nonce: Option<String>,
    },
}

impl EscapeCodes {
    fn osc_133_encode_string(s: &str) -> String {
        // encoded by %q means the encoding produced by the %q format to printf in bash and similar shells
        if s.is_empty() {
            return "''".to_string();
        }

        let needs_quoting = s.chars().any(|c| {
            !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '@' | '%' | '+' | ',' | '.' | '/' | '-' | '_' | ':')
        });

        if !needs_quoting {
            return s.to_string();
        }

        let mut result = String::from("$'");
        for c in s.chars() {
            match c {
                '\x07' => result.push_str("\\a"),
                '\x08' => result.push_str("\\b"),
                '\t' => result.push_str("\\t"),
                '\n' => result.push_str("\\n"),
                '\x0B' => result.push_str("\\v"),
                '\x0C' => result.push_str("\\f"),
                '\r' => result.push_str("\\r"),
                '\x1B' => result.push_str("\\E"),
                '\'' => result.push_str("\\'"),
                '\\' => result.push_str("\\\\"),
                c if (c as u32) < 0x20 || c as u32 == 0x7F => {
                    result.push_str(&format!("\\x{:02x}", c as u32));
                }
                c => result.push(c),
            }
        }
        result.push('\'');
        result
    }

    fn vsc_encode_string(s: &str) -> String {
        // The command line can escape ASCII characters using the \xAB format, where AB are the hexadecimal representation of the character code (case insensitive), and escape the \ character using \\. It's required to escape semi-colon (0x3b) and characters 0x20 and below and this is particularly important for new line and semi-colon.
        s.chars()
            .map(|c| {
                if c == '\\' {
                    "\\\\".to_string()
                } else if c as u32 <= 0x20 || c == ';' {
                    format!("\\x{:02x}", c as u32)
                } else {
                    c.to_string()
                }
            })
            .collect()
    }
}

impl std::fmt::Display for EscapeCodes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shell_pgrp = shell::backend().shell_pgrp();

        match self {
            // OSC 7
            EscapeCodes::CurrentDirectory { host, path } => {
                write!(
                    f,
                    "\x1b]7;file://{}{}\x1b\\",
                    percent_encode_uri_part(host, b""),
                    percent_encode_uri_part(path, b"/")
                )
            }
            // kitty reads this form's path verbatim (no percent-decoding), so only the
            // control characters that would end the sequence early are dropped.
            EscapeCodes::KittyCurrentDirectory { host, path } => {
                let strip = |s: &str| s.chars().filter(|c| !c.is_control()).collect::<String>();
                write!(
                    f,
                    "\x1b]7;kitty-shell-cwd://{}{}\x1b\\",
                    strip(host),
                    strip(path)
                )
            }
            // OSC 2
            EscapeCodes::WindowTitle { title } => {
                let title: String = title.chars().filter(|c| !c.is_control()).collect();
                write!(f, "\x1b]2;{}\x1b\\", title)
            }
            // OSC 133
            // We want to enable click_events=1 so that we get a mouse event instead
            // of the terminal sending keystrokes to try and move the cursor.
            EscapeCodes::PromptStart { .. } => {
                write!(
                    f,
                    "\x1b]133;A;click_events=1;redraw=1;aid={}\x1b\\",
                    shell_pgrp
                )
            }
            EscapeCodes::PromptEnd { .. } => f.write_str("\x1b]133;B\x1b\\"),
            EscapeCodes::PreExecution { commandline } => match commandline {
                Some(cmd) => write!(
                    f,
                    "\x1b]133;C;cmdline={}\x1b\\",
                    EscapeCodes::osc_133_encode_string(cmd)
                ),
                None => f.write_str("\x1b]133;C\x1b\\"),
            },
            EscapeCodes::ExecutionFinished { exit_code, .. } => match exit_code {
                Some(code) => write!(f, "\x1b]133;D;{};aid={}\x1b\\", code, shell_pgrp),
                None => write!(f, "\x1b]133;D;aid={}\x1b\\", shell_pgrp),
            },

            // OSC 633
            EscapeCodes::VscProperties {
                cwd,
                has_rich_command_detection,
            } => write!(
                f,
                "\x1b]633;P;Cwd={};HasRichCommandDetection={}\x1b\\",
                EscapeCodes::vsc_encode_string(cwd),
                if *has_rich_command_detection {
                    "True"
                } else {
                    "False"
                }
            ),
            EscapeCodes::VscPromptStart { .. } => f.write_str("\x1b]633;A\x1b\\"),
            EscapeCodes::VscPromptEnd { .. } => f.write_str("\x1b]633;B\x1b\\"),
            EscapeCodes::VscPreExecution => f.write_str("\x1b]633;C\x1b\\"),
            EscapeCodes::VscExecutionFinished { exit_code, .. } => match exit_code {
                Some(code) => write!(f, "\x1b]633;D;{}\x1b\\", code),
                None => f.write_str("\x1b]633;D\x1b\\"),
            },
            EscapeCodes::VscCommandLine {
                commandline, nonce, ..
            } => match nonce {
                Some(n) => write!(
                    f,
                    "\x1b]633;E;{};{}\x1b\\",
                    EscapeCodes::vsc_encode_string(commandline),
                    n
                ),
                None => write!(
                    f,
                    "\x1b]633;E;{}\x1b\\",
                    EscapeCodes::vsc_encode_string(commandline)
                ),
            },
        }
    }
}

pub fn write_startup_codes(exit_code: i32, hostname: &str, cwd: &str) -> std::io::Result<()> {
    let codes: Vec<EscapeCodes> = if term_info::is_vscode() {
        vec![
            EscapeCodes::VscExecutionFinished {
                exit_code: Some(exit_code),
            },
            EscapeCodes::VscProperties {
                cwd: cwd.to_string(),
                has_rich_command_detection: true,
            },
        ]
    } else {
        vec![
            EscapeCodes::ExecutionFinished {
                exit_code: Some(exit_code),
            },
            EscapeCodes::CurrentDirectory {
                host: hostname.to_string(),
                path: cwd.to_string(),
            },
            EscapeCodes::KittyCurrentDirectory {
                host: hostname.to_string(),
                path: cwd.to_string(),
            },
        ]
    };
    write_escape_codes(&codes)
}

pub fn write_after_rendering_codes(
    writer: &mut impl std::io::Write,
    prev_prompt_start: Option<RelativePosition>,
    prev_prompt_end: Option<RelativePosition>,
    new_prompt_start: Option<RelativePosition>,
    new_prompt_end: Option<RelativePosition>,
    is_running: bool,
) -> std::io::Result<()> {
    let mut codes: Vec<EscapeCodes> = vec![];

    let effective_start = new_prompt_start
        .filter(|&ps| !is_running || prev_prompt_start.is_none_or(|prev| prev != ps));
    let effective_end =
        new_prompt_end.filter(|&pe| !is_running || prev_prompt_end.is_none_or(|prev| prev != pe));

    if term_info::is_vscode() {
        if let Some(rel_pos) = effective_start {
            codes.push(EscapeCodes::VscPromptStart { rel_pos });
        }
        if let Some(rel_pos) = effective_end {
            codes.push(EscapeCodes::VscPromptEnd { rel_pos });
        }
    } else {
        if let Some(rel_pos) = effective_start {
            codes.push(EscapeCodes::PromptStart { rel_pos });
        }
        if let Some(rel_pos) = effective_end {
            codes.push(EscapeCodes::PromptEnd { rel_pos });
        }
    }

    write_escape_codes_to(writer, &codes)
}

pub fn write_on_exit_codes(commandline: Option<&str>) -> std::io::Result<()> {
    let codes: Vec<EscapeCodes> = if term_info::is_vscode() {
        let nonce = shell::backend().env_var("VSCODE_NONCE");
        log::trace!("vscode_nonce: {:?}", nonce);
        match commandline {
            Some(cmd) => vec![
                EscapeCodes::VscCommandLine {
                    commandline: cmd.to_string(),
                    nonce,
                },
                EscapeCodes::VscPreExecution,
            ],
            None => vec![EscapeCodes::VscPreExecution],
        }
    } else {
        vec![EscapeCodes::PreExecution {
            commandline: commandline.map(|s| s.to_string()),
        }]
    };
    write_escape_codes(&codes)
}

pub fn write_escape_codes(codes: &[EscapeCodes]) -> std::io::Result<()> {
    write_escape_codes_to(&mut std::io::stdout(), codes)
}

pub fn write_escape_codes_to(
    queue: &mut impl std::io::Write,
    codes: &[EscapeCodes],
) -> std::io::Result<()> {
    use termina::OneBased;
    use termina::escape::csi::{Csi, Cursor};

    // DECSC / DECRC (`ESC 7` / `ESC 8`), not SCOSC / SCORC (`CSI s` / `CSI u`).
    // mosh and some other emulators ignore `CSI s`, which left the cursor at
    // the prompt-start mark and drew the input above the prompt
    // (HalFrgrd/flyline#1070). `CSI s` also means DECSLRM when left/right
    // margin mode is on. Every VT100-compatible terminal has DECSC.
    const SAVE_CURSOR: &str = "\x1b7";
    const RESTORE_CURSOR: &str = "\x1b8";

    if codes.is_empty() {
        return Ok(());
    }

    for code in codes {
        let rel_pos = match code {
            EscapeCodes::PromptStart { rel_pos }
            | EscapeCodes::PromptEnd { rel_pos }
            | EscapeCodes::VscPromptStart { rel_pos }
            | EscapeCodes::VscPromptEnd { rel_pos } => Some(*rel_pos),
            _ => None,
        };
        if let Some(rel_pos) = rel_pos {
            log::trace!(
                "Moving cursor relative (col={}, dy={}) for escape code: {:?}",
                rel_pos.col,
                rel_pos.dy,
                code
            );
            queue.write_all(SAVE_CURSOR.as_bytes())?;
            if rel_pos.dy < 0 {
                write!(queue, "{}", Csi::Cursor(Cursor::Up((-rel_pos.dy) as u32)))?;
            } else if rel_pos.dy > 0 {
                write!(queue, "{}", Csi::Cursor(Cursor::Down(rel_pos.dy as u32)))?;
            }
            write!(
                queue,
                "{}",
                Csi::Cursor(Cursor::CharacterAbsolute(OneBased::from_zero_based(
                    rel_pos.col
                )))
            )?;
            log::trace!("Writing escape code: {:?}", code);
            write!(queue, "{}", code)?;
            queue.write_all(RESTORE_CURSOR.as_bytes())?;
        } else {
            log::trace!("Writing escape code: {:?}", code);
            write!(queue, "{}", code)?;
        }
    }
    queue.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_directory_is_a_percent_encoded_file_uri() {
        let path = "/tmp/a b/#1?/100%/\u{e9}\x1b]2;pwned\x07";
        let osc7 = EscapeCodes::CurrentDirectory {
            host: "my host".into(),
            path: path.into(),
        }
        .to_string();
        assert_eq!(
            osc7,
            "\x1b]7;file://my%20host/tmp/a%20b/%231%3F/100%25/%C3%A9%1B%5D2%3Bpwned%07\x1b\\"
        );

        let kitty = EscapeCodes::KittyCurrentDirectory {
            host: "h".into(),
            path: path.into(),
        }
        .to_string();
        assert_eq!(
            kitty,
            "\x1b]7;kitty-shell-cwd://h/tmp/a b/#1?/100%/\u{e9}]2;pwned\x1b\\"
        );
    }

    #[test]
    fn prompt_marks_save_and_restore_the_cursor_with_decsc() {
        let codes = [
            EscapeCodes::PromptStart {
                rel_pos: RelativePosition { col: 0, dy: -2 },
            },
            EscapeCodes::PromptEnd {
                rel_pos: RelativePosition { col: 2, dy: 0 },
            },
        ];
        let mut out = Vec::new();
        write_escape_codes_to(&mut out, &codes).unwrap();
        let out = String::from_utf8(out).unwrap();

        assert!(
            !out.contains("\x1b[s"),
            "SCOSC is not supported by mosh: {out:?}"
        );
        assert!(
            !out.contains("\x1b[u"),
            "SCORC is not supported by mosh: {out:?}"
        );
        assert_eq!(out.matches("\x1b7").count(), 2, "{out:?}");
        assert_eq!(out.matches("\x1b8").count(), 2, "{out:?}");
        // Each mark is bracketed: save, move, mark, restore.
        let start = out.find("\x1b]133;A").unwrap();
        assert!(out[..start].ends_with("\x1b[2A\x1b[G"), "{out:?}");
        assert!(out[start..].contains("\x1b8"), "{out:?}");
    }
}
