//! Keeping space-prefixed commands out of the JSONL history.
//!
//! Bash drops a line that starts with a space from history when `HISTCONTROL`
//! contains `ignorespace` or `ignoreboth`. flyline asks Bash the same question
//! through `check_add_history`, but bash-preexec (bundled by `atuin init bash`)
//! rewrites `HISTCONTROL` on its first prompt: it removes `ignorespace` and turns
//! `ignoreboth` into `ignoredups`, so that its `history 1` lookup still sees the
//! command. After that rewrite Bash no longer filters these lines, and flyline
//! would write them to the JSONL file.
//!
//! flyline therefore remembers whether `HISTCONTROL` asked for `ignorespace`
//! before bash-preexec changed it, and keeps honoring it while bash-preexec is
//! loaded.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::shell;

/// Set once `HISTCONTROL` has been seen with `ignorespace` or `ignoreboth`.
static IGNORESPACE_SEEN: AtomicBool = AtomicBool::new(false);

/// Whether a `HISTCONTROL` value tells Bash to drop lines that start with a space.
pub fn histcontrol_ignores_space(histcontrol: &str) -> bool {
    histcontrol
        .split(':')
        .any(|value| value == "ignorespace" || value == "ignoreboth")
}

/// Records whether the current `HISTCONTROL` asks for `ignorespace`.
///
/// flyline calls this when the builtin loads and on each `flyline` command, which
/// in a typical `.bashrc` runs before bash-preexec rewrites `HISTCONTROL` on the
/// first prompt.
pub fn note_histcontrol() {
    if shell::backend()
        .env_var("HISTCONTROL")
        .is_some_and(|value| histcontrol_ignores_space(&value))
    {
        IGNORESPACE_SEEN.store(true, Ordering::Relaxed);
    }
}

fn bash_preexec_loaded() -> bool {
    let backend = shell::backend();
    backend.env_var("bash_preexec_imported").is_some() || backend.env_var("__bp_imported").is_some()
}

/// Whether flyline should keep `command` out of the JSONL history because it
/// starts with a space and the user asked for `ignorespace`.
pub fn should_ignore_command(command: &str) -> bool {
    should_ignore(
        command,
        shell::backend().env_var("HISTCONTROL").as_deref(),
        IGNORESPACE_SEEN.load(Ordering::Relaxed),
        bash_preexec_loaded(),
    )
}

fn should_ignore(
    command: &str,
    histcontrol: Option<&str>,
    ignorespace_seen: bool,
    bash_preexec_loaded: bool,
) -> bool {
    if !command.starts_with(' ') {
        return false;
    }
    histcontrol.is_some_and(histcontrol_ignores_space) || (ignorespace_seen && bash_preexec_loaded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histcontrol_values() {
        assert!(histcontrol_ignores_space("ignorespace"));
        assert!(histcontrol_ignores_space("ignoreboth"));
        assert!(histcontrol_ignores_space("ignoreboth:erasedups"));
        assert!(histcontrol_ignores_space("erasedups:ignorespace"));
        assert!(!histcontrol_ignores_space(""));
        assert!(!histcontrol_ignores_space("ignoredups"));
        // What bash-preexec leaves behind from "ignoreboth:erasedups".
        assert!(!histcontrol_ignores_space("ignoredups::erasedups"));
    }

    #[test]
    fn only_space_prefixed_commands_are_ignored() {
        assert!(should_ignore(
            " echo secret",
            Some("ignoreboth"),
            false,
            false
        ));
        assert!(!should_ignore(
            "echo public",
            Some("ignoreboth"),
            true,
            true
        ));
        assert!(!should_ignore("", Some("ignoreboth"), true, true));
    }

    #[test]
    fn current_histcontrol_decides_without_bash_preexec() {
        assert!(should_ignore(" ls", Some("ignorespace"), false, false));
        assert!(!should_ignore(" ls", Some("ignoredups"), false, false));
        assert!(!should_ignore(" ls", None, false, false));
        // Without bash-preexec, a user who drops ignorespace mid-session gets
        // space-prefixed commands recorded again.
        assert!(!should_ignore(" ls", Some("ignoredups"), true, false));
    }

    #[test]
    fn remembered_ignorespace_survives_bash_preexec_rewrite() {
        assert!(should_ignore(
            " ls",
            Some("ignoredups::erasedups"),
            true,
            true
        ));
        assert!(should_ignore(" ls", None, true, true));
        // bash-preexec alone does not imply ignorespace.
        assert!(!should_ignore(" ls", Some("ignoredups"), false, true));
    }
}
