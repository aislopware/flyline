# Vendored forks

flyline needs changes to these crates that upstream does not have. They used to
be git dependencies on HalFrgrd's forks; they live here so this repository can
update them itself.

| Directory | Upstream | Base commit | flyline's changes |
|---|---|---|---|
| `ratatui/` | [ratatui/ratatui](https://github.com/ratatui/ratatui) `main` | `e665c36cb1` | Inline viewport: resizable, no cursor-position query, absolute positions for mouse events, synchronized output, autowrap off, erase-instead-of-space for reflow, wide-character cursor tracking, no redraw sequences when nothing changed (HalFrgrd/ratatui `inline_viewport_improvments`, 30 commits) |
| `termina/` | [helix-editor/termina](https://github.com/helix-editor/termina) `master` | `4efcdc689e` | SGR mouse reports, text pixel size and OSC 52 responses, a long-lived event reader that keeps unread stdin, Ghostty DA2 deadlock fix, raw device attributes, kitty base-layout key (HalFrgrd/termina, 13 commits; two are upstream PRs #31 and #32) |
| `skim/` | [skim-rs/skim](https://github.com/skim-rs/skim) `master` | `6fb2cc5feb` | A `tui` feature, so `default-features = false, features = ["algos_only"]` builds only the matchers, without the TUI and runtime stack (HalFrgrd/skim, 1 commit) |

Each directory is a `git subtree --squash`. It holds upstream's tree at the
base commit plus one commit with flyline's changes. Because the squash records
upstream's commit and not the fork's, a later `git subtree pull` from upstream
merges upstream's changes into flyline's instead of reverting them.

The root `Cargo.toml` uses these as path dependencies and `[patch.crates-io]`s
`ratatui-core` and `termina`, so crates.io dependents link the same copies. The
workspace `exclude`s `vendor/`. The crates keep their own workspaces, lints and
`rustfmt.toml`, so use `cargo fmt` (not `cargo fmt --all`) and
`cargo clippy --workspace` for flyline's own code.

## Update from upstream

```bash
git subtree pull --prefix vendor/termina https://github.com/helix-editor/termina.git master --squash
git subtree pull --prefix vendor/skim    https://github.com/skim-rs/skim.git        master --squash
git subtree pull --prefix vendor/ratatui https://github.com/ratatui/ratatui.git     main   --squash
```

1. Resolve conflicts in favour of keeping flyline's behaviour.
2. Run `cargo update -w`, then the checks in AGENTS.md and the Docker bash integration tests.
3. Update the base commit in the table above.

## Change a fork

Edit the files here and commit with a `vendor(<crate>):` prefix. Changes worth
upstreaming should also be sent as PRs to the upstream repository.
