//! Host shell abstraction layer.
//!
//! `ShellBackend` provides a clean seam over shell-specific runtime operations,
//! decoupling the editor UI, completion engine, and prompt managers from direct
//! GNU Bash C FFI symbols.
//!
//! By default, flyline operates with [`BashBackend`], which communicates directly
//! with GNU Bash's runtime when loaded as a dynamic builtin (`libflyline.so`).
//!
//! Downstream fork maintainers wishing to support additional shells (such as Zsh
//! via an out-of-process standalone editor or ZLE widget) can implement [`ShellBackend`]
//! for their shell (e.g. `pub mod zsh;` containing `ZshBackend`) and register it:

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::history::HistoryEntry;

pub use crate::completions::{
    CompletionFlags, ProgrammableCompleteReturn, detect_and_convert_inline_descriptions,
};
pub use crate::grammar::{
    QuoteType, dequoting_function_rust, find_quote_type, quoting_function_rust,
};
pub use crate::path::{ExecutablesOnPath, PathScanPayload};

#[derive(Debug, PartialEq, Eq, Hash, Clone, serde::Serialize, serde::Deserialize)]
pub enum CommandWordInfo {
    Unknown {
        command: String,
    },
    Alias {
        command: String,
        expansion: String,
    },
    Keyword {
        command: String,
        usage: Option<String>,
    },
    Function {
        command: String,
        source_file: Option<String>,
        line: Option<i32>,
    },
    Builtin {
        command: String,
        usage: Option<String>,
    },
    File {
        command: String,
        path: String,
    },
    EnvVar {
        name: String,
    },
}

impl CommandWordInfo {
    pub fn is_known(&self) -> bool {
        !matches!(self, CommandWordInfo::Unknown { .. })
    }

    pub fn command(&self) -> &str {
        match self {
            CommandWordInfo::Unknown { command } => command,
            CommandWordInfo::Alias { command, .. } => command,
            CommandWordInfo::Keyword { command, .. } => command,
            CommandWordInfo::Function { command, .. } => command,
            CommandWordInfo::Builtin { command, .. } => command,
            CommandWordInfo::File { command, .. } => command,
            CommandWordInfo::EnvVar { name } => name,
        }
    }

    pub fn to_description(&self) -> String {
        match self {
            CommandWordInfo::Unknown { .. } => "unknown".to_string(),
            CommandWordInfo::Alias { expansion, .. } => format!("alias: {}", expansion),
            CommandWordInfo::Keyword { command, usage } => {
                if let Some(u) = usage {
                    format!("keyword: {}", u)
                } else {
                    format!("keyword: {}", command)
                }
            }
            CommandWordInfo::Builtin { command, usage } => {
                if let Some(u) = usage {
                    format!("builtin: {}", u)
                } else {
                    format!("builtin: {}", command)
                }
            }
            CommandWordInfo::File { path, .. } => path.clone(),
            CommandWordInfo::EnvVar { .. } => "env var".to_string(),
            CommandWordInfo::Function {
                source_file, line, ..
            } => match (source_file, line) {
                (Some(file), Some(l)) => format!("function {}:{}", file, l),
                (Some(file), None) => format!("function {}", file),
                (None, Some(l)) => format!("function :{}", l),
                (None, None) => "function".to_string(),
            },
        }
    }
}

/// Shell builtin exit code constants (GNU Bash compatible).
#[repr(i32)]
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinExitCode {
    ExecutionSuccess = 0,
    BadSyntax = 257,    // shell syntax error
    Usage = 258,        // syntax error in usage
    RedirFail = 259,    // redirection failed
    BadAssign = 260,    // variable assignment error
    ExpFail = 261,      // word expansion failed
    DiskFallback = 262, // fall back to disk command from builtin
    UtilError = 263,    // Posix special builtin utility error
}

pub mod test_backend;
pub use test_backend::TestBackend;

/// The host shell that flyline is embedded in or driven by.
///
/// Implementors must be `Sync`: a single backend is shared by `&'static`
/// reference across flyline's threads.
pub trait ShellBackend: Sync {
    /// Human-readable identifier for the host shell (e.g. `"bash"`, `"test"`, `"zsh"`).
    fn name(&self) -> &'static str;

    /// Working directory as the shell sees it (used for prompt + OSC 7 reporting).
    fn cwd(&self) -> String;

    /// Retrieve the value of an environment / shell variable.
    fn env_var(&self, name: &str) -> Option<String>;

    /// Set an exported environment variable.
    fn export_env_var(&self, name: &str, value: &str) -> anyhow::Result<()>;

    /// Unset an environment variable.
    fn unset_env_var(&self, name: &str) -> anyhow::Result<()>;

    /// Return the host machine's hostname as reported by the shell.
    fn hostname(&self) -> String;

    /// Format a variable assignment (e.g. `"VAR=val"` with quoting if needed).
    fn format_var(&self, name: &str) -> String;

    /// List all variable names matching the given prefix.
    fn vars_with_prefix(&self, prefix: &str) -> Vec<String>;

    /// Fully expand a shell path string (quotes, tilde, vars, subshell) into an absolute path.
    fn expand_path(&self, path: &str) -> String;

    /// Expand a shell word into a filename (quote removal, tilde and variable expansion).
    fn expand_filename(&self, filename: &str) -> String;

    /// Resolve a completion candidate string to a filesystem path.
    ///
    /// Programmable completions (e.g. `compgen -f` / `_filedir`) return raw,
    /// unquoted filenames that may contain quotes, backslashes, backticks, or `$`
    /// (such as ``a`b.txt`` or `it's.txt`). Check whether the literal path exists
    /// before falling back to shell word expansion (for `~/...` or `$HOME/...`).
    fn resolve_completion_path(&self, candidate: &str) -> PathBuf {
        if !candidate.is_empty() {
            let literal = if std::path::Path::new(candidate).is_absolute() {
                PathBuf::from(candidate)
            } else {
                PathBuf::from(self.cwd()).join(candidate)
            };
            if literal.symlink_metadata().is_ok() {
                return literal;
            }
        }
        PathBuf::from(self.expand_path(candidate))
    }

    /// Expand prompt escape sequences (`\u`, `\h`, `\w`, etc.).
    fn decode_prompt(&self, raw: &str, is_prompt: bool) -> Option<String>;

    /// Look up an alias definition by name.
    fn find_alias(&self, cmd: &str) -> Option<String>;

    /// Classify a command name (alias, keyword, function, builtin, file, unknown).
    fn command_info(&self, cmd: &str) -> CommandWordInfo;

    /// Run host programmable completions for a command line.
    fn run_programmable_completions(
        &self,
        full_command: &str,
        command_word: &str,
        word_under_cursor: &str,
        cursor_byte_pos: usize,
        word_under_cursor_byte_end: usize,
    ) -> anyhow::Result<ProgrammableCompleteReturn>;

    /// Return all known first-word command candidates.
    fn possible_command_words(&self) -> Vec<CommandWordInfo>;

    /// Evaluate an arbitrary shell script string inside the host shell context.
    fn evaluate_shell_string(&self, script: &str) -> anyhow::Result<()>;

    /// Invalidate any host shell caches (e.g. before prompt redraw).
    fn reset_caches(&self) {}

    /// Pre-warm completion and metadata caches in background/foreground.
    fn warm_completion_caches(&self) {}

    /// Read any pending terminating signal from host shell globals.
    fn read_terminating_signal(&self) -> libc::c_int {
        0
    }

    /// Check if the host shell has any pending signal traps queued.
    fn has_pending_traps(&self) -> bool {
        false
    }

    /// Execute any pending signal traps in the host shell.
    fn run_pending_traps(&self) {}

    /// Resolve the filesystem path where generated completions should be written.
    fn resolve_completion_script_path(
        &self,
        command_word: &str,
        flycomp_output: Option<&str>,
    ) -> PathBuf {
        let file_name = std::path::Path::new(command_word)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(command_word);
        let output_dir = flycomp_output.unwrap_or("~/.local/share/bash-completion/completions/");
        let expanded = self.expand_path(output_dir);
        std::path::Path::new(&expanded).join(file_name)
    }

    /// Resolve output path and persist a newly generated completion script.
    fn resolve_and_write_completion_script(
        &self,
        command_word: &str,
        script: &str,
        flycomp_output: Option<&str>,
    ) -> Result<PathBuf, std::io::Error> {
        let write_path = self.resolve_completion_script_path(command_word, flycomp_output);
        if let Some(parent) = write_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if write_path.exists() {
            let now = chrono::Local::now();
            let datetime_str = now.format("%Y%m%d_%H%M%S").to_string();
            let file_name = write_path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(command_word);
            let backup_name = format!("{}_backup_{}", file_name, datetime_str);
            if let Some(parent) = write_path.parent() {
                let backup_path = parent.join(backup_name);
                std::fs::rename(&write_path, &backup_path)?;
            }
        }
        std::fs::write(&write_path, script)?;
        Ok(write_path)
    }

    /// Parse history entries currently loaded in shell memory.
    fn parse_history_from_memory(&self) -> Vec<HistoryEntry> {
        Vec::new()
    }

    /// Return the exit status of the last executed foreground command.
    fn last_command_exit_status(&self) -> i32 {
        0
    }

    /// Return current multiline command line count in shell parser.
    fn multiline_command_count(&self) -> i32 {
        0
    }

    /// Return host shell process group ID.
    fn shell_pgrp(&self) -> libc::pid_t {
        0
    }

    /// True if the shell has `autocd` enabled.
    fn is_autocd_enabled(&self) -> bool {
        false
    }

    /// True if the shell has `shopt -s dotglob` enabled.
    fn is_dotglob_enabled(&self) -> bool {
        self.env_var("BASHOPTS")
            .is_some_and(|opts| opts.split(':').any(|opt| opt == "dotglob"))
    }

    /// Called when the line editor UI enters and prepares terminal mode.
    fn prep_terminal(&self) {}

    /// Called when the line editor UI exits and restores terminal mode.
    fn deprep_terminal(&self) {}
}

#[cfg(not(test))]
pub mod bash;

#[cfg(not(test))]
pub use bash::BashBackend;

#[cfg(not(test))]
static BASH: BashBackend = BashBackend;
static ACTIVE: OnceLock<&'static dyn ShellBackend> = OnceLock::new();

#[cfg(not(test))]
fn init_backend() -> &'static dyn ShellBackend {
    &BASH
}

#[cfg(test)]
fn init_backend() -> &'static dyn ShellBackend {
    &*test_backend::TEST_BACKEND
}

/// Expand a shell word into a literal filename safely in Rust without invoking
/// Bash's `expand_string_to_string`.
///
/// Supports:
/// - Leading unquoted tilde expansion (`~`, `~/...`, `~+`, `~-`, `~user`, `~user/...`)
/// - Shell parameter expansion (`$NAME`, `${NAME}`) in unquoted and double-quoted text
/// - Quote removal (`'...'`, `"..."`, and backslash escapes following Bash rules)
///
/// Deliberately does **not** perform command substitution (`` `...` ``, `$(...)`),
/// arithmetic expansion, or process substitution. Calling Bash's
/// `expand_string_to_string` on an unclosed backtick (e.g. a filename like
/// ``a`b.txt``) triggers `jump_to_top_level` (`siglongjmp`), which unwinds out of
/// the completion subshell and deadlocks on `FLYLINE_INSTANCE_PTR` (#1074); and
/// paired backticks / `$(...)` in filenames would execute arbitrary commands on Tab.
pub fn expand_shell_filename<F>(filename: &str, lookup_env: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let mut result = String::with_capacity(filename.len());
    let mut rest = filename;

    // Leading unquoted tilde expansion: up to the first `/` (or end of word),
    // provided the tilde prefix itself contains no quoting or escapes.
    if let Some(after_tilde) = rest.strip_prefix('~') {
        let prefix_end = after_tilde.find('/').unwrap_or(after_tilde.len());
        let tilde_prefix = &after_tilde[..prefix_end];
        let has_quote_or_escape = tilde_prefix
            .chars()
            .any(|c| matches!(c, '\'' | '"' | '\\' | '$'));
        if !has_quote_or_escape {
            let expanded_tilde = match tilde_prefix {
                "" => lookup_env("HOME"),
                "+" => lookup_env("PWD"),
                "-" => lookup_env("OLDPWD"),
                user => crate::users::get_all_users()
                    .iter()
                    .find(|u| u.username == user)
                    .map(|u| u.home_dir.clone()),
            };
            if let Some(dir) = expanded_tilde {
                result.push_str(&dir);
                rest = &after_tilde[prefix_end..];
            }
        }
    }

    let mut chars = rest.chars().peekable();
    let mut quote: Option<char> = None;

    while let Some(c) = chars.next() {
        match quote {
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                } else {
                    result.push(c);
                }
            }
            Some('"') => match c {
                '"' => quote = None,
                '\\' => match chars.peek() {
                    // Inside double quotes, backslash only escapes these characters;
                    // backslash-newline is a line continuation (both removed).
                    Some('\n') => {
                        chars.next();
                    }
                    Some('$' | '`' | '"' | '\\') => {
                        result.push(chars.next().unwrap());
                    }
                    _ => result.push('\\'),
                },
                '$' => expand_shell_var(&mut chars, &lookup_env, &mut result),
                _ => result.push(c),
            },
            Some(_) => unreachable!(),
            None => match c {
                '\'' | '"' => quote = Some(c),
                '\\' => match chars.next() {
                    Some('\n') | None => {}
                    Some(escaped) => result.push(escaped),
                },
                '$' => expand_shell_var(&mut chars, &lookup_env, &mut result),
                _ => result.push(c),
            },
        }
    }

    result
}

/// Expand `$NAME` or `${NAME}` into `out`. Unknown names expand to empty.
/// Unrecognised `$` forms (like `$`, `$(...)`, `$1`, `${invalid}`) are left literal
/// without executing anything.
fn expand_shell_var<F>(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    lookup_env: &F,
    out: &mut String,
) where
    F: Fn(&str) -> Option<String>,
{
    if chars.peek() == Some(&'{') {
        // Peek ahead by cloning the iterator so an unclosed or non-plain `${...}`
        // (such as `${a` or `${foo:-bar}`) does not swallow the rest of the path.
        let mut probe = chars.clone();
        probe.next(); // consume '{'
        let mut name = String::new();
        let mut closed = false;
        let mut valid = true;
        for vc in probe.by_ref() {
            if vc == '}' {
                closed = true;
                break;
            }
            let ok = if name.is_empty() {
                vc.is_ascii_alphabetic() || vc == '_'
            } else {
                vc.is_ascii_alphanumeric() || vc == '_'
            };
            if !ok {
                valid = false;
                break;
            }
            name.push(vc);
        }
        if closed && valid && !name.is_empty() {
            *chars = probe;
            if let Some(val) = lookup_env(&name) {
                out.push_str(&val);
            }
            return;
        }
        out.push('$');
        return;
    }

    if chars
        .peek()
        .is_some_and(|c| c.is_ascii_alphabetic() || *c == '_')
    {
        let mut name = String::new();
        while let Some(&vc) = chars.peek() {
            if vc.is_ascii_alphanumeric() || vc == '_' {
                name.push(vc);
                chars.next();
            } else {
                break;
            }
        }
        if let Some(val) = lookup_env(&name) {
            out.push_str(&val);
        }
    } else {
        out.push('$');
    }
}

/// The active host shell backend. Defaults to `BashBackend`.
pub fn backend() -> &'static dyn ShellBackend {
    *ACTIVE.get_or_init(init_backend)
}

/// Helper for tests to toggle `autocd` behavior on the test/mock backend.
#[cfg(test)]
pub fn set_test_autocd(enabled: bool) {
    test_backend::TEST_BACKEND.set_autocd(enabled);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_test_backend_identity() {
        assert_eq!(backend().name(), "test");
    }

    #[test]
    fn default_test_backend_defaults() {
        assert_eq!(backend().hostname(), "test-host");
        assert_eq!(
            backend().cwd(),
            std::env::current_dir().unwrap().to_string_lossy()
        );
        assert_eq!(backend().env_var("USER"), Some("john".to_string()));
        assert_eq!(backend().env_var("FLYLINE_DEFINITELY_UNSET_VAR"), None);
    }

    #[test]
    fn default_test_backend_mock_functionality() {
        let tb = TestBackend::new();
        assert_eq!(tb.name(), "test");
        assert_eq!(tb.hostname(), "test-host");
        assert_eq!(tb.env_var("USER"), Some("john".to_string()));

        tb.set_env("FOO", "BAR");
        assert_eq!(tb.env_var("FOO"), Some("BAR".to_string()));
        assert_eq!(tb.format_var("FOO"), "FOO=BAR");

        tb.set_alias("ll", "ls -la");
        assert_eq!(tb.find_alias("ll"), Some("ls -la".to_string()));
        assert!(matches!(
            tb.command_info("ll"),
            CommandWordInfo::Alias { .. }
        ));

        assert_eq!(tb.expand_path("~/foo"), "/home/john/foo");
    }

    #[test]
    fn test_backend_expansion_respects_quoting() {
        let tb = TestBackend::new();
        tb.set_env("HOME", "/home/test");
        tb.set_env("X", "value");

        assert_eq!(tb.expand_filename("$X"), "value");
        assert_eq!(tb.expand_filename("${X}"), "value");
        assert_eq!(tb.expand_filename("\"$X\""), "value");
        assert_eq!(tb.expand_filename("a\\$X"), "a$X");
        assert_eq!(tb.expand_filename("a\"\\$X\""), "a$X");
        assert_eq!(tb.expand_filename("a'$X'"), "a$X");
        assert_eq!(tb.expand_filename("x/'$X'"), "x/$X");
        assert_eq!(tb.expand_filename("~"), "/home/test");
        assert_eq!(tb.expand_filename("~/foo"), "/home/test/foo");
        assert_eq!(tb.expand_filename("\"~\""), "~");
        assert_eq!(tb.expand_filename("'~'"), "~");
        assert_eq!(tb.expand_filename("\\~"), "~");
        assert_eq!(tb.expand_path("'$X'"), format!("{}/$X", tb.cwd()));
    }

    #[test]
    fn expand_shell_filename_backticks_and_cmdsubst_are_literal() {
        let tb = TestBackend::new();
        tb.set_env("HOME", "/home/test");
        tb.set_env("PWD", "/work");
        tb.set_env("OLDPWD", "/prev");

        // Unclosed and paired backticks must stay literal (never execute or longjmp).
        assert_eq!(tb.expand_filename("a`b.txt"), "a`b.txt");
        assert_eq!(tb.expand_filename("a\\`b.txt"), "a`b.txt");
        assert_eq!(tb.expand_filename("\"a\\`b.txt\""), "a`b.txt");
        assert_eq!(tb.expand_filename("'a`b.txt'"), "a`b.txt");
        assert_eq!(tb.expand_filename("a`echo pwned`b.txt"), "a`echo pwned`b.txt");

        // Command substitutions and unclosed braces must also stay literal.
        assert_eq!(tb.expand_filename("$(echo hi).txt"), "$(echo hi).txt");
        assert_eq!(tb.expand_filename("${unclosed"), "${unclosed");
        assert_eq!(tb.expand_filename("${X:-default}"), "${X:-default}");

        // Tilde ~+ and ~- expansion.
        assert_eq!(tb.expand_filename("~+/sub"), format!("{}/sub", tb.cwd()));
        assert_eq!(tb.expand_filename("~-/sub"), "/prev/sub");
    }

    #[test]
    fn resolve_completion_path_prefers_existing_literal_filename() {
        let dir = std::env::temp_dir().join(format!(
            "flyline_resolve_literal_{}",
            rand::random::<u32>()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["a`b.txt", "it's.txt", "a\\b.txt", "$HOME.txt"] {
            std::fs::write(dir.join(name), "").unwrap();
        }

        let tb = TestBackend::new();
        for name in ["a`b.txt", "it's.txt", "a\\b.txt", "$HOME.txt"] {
            let abs = dir.join(name);
            let resolved = tb.resolve_completion_path(abs.to_str().unwrap());
            assert_eq!(resolved, abs, "literal path should win for {name}");
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_backend_traps_execution() {
        let tb = TestBackend::new();
        assert!(!tb.has_pending_traps());

        tb.queue_trap("export MY_TRAP_VAR=triggered_value");
        assert!(tb.has_pending_traps());

        tb.run_pending_traps();
        assert!(!tb.has_pending_traps());
        assert_eq!(
            tb.env_var("MY_TRAP_VAR"),
            Some("triggered_value".to_string())
        );
    }
}
