# Vendored forks

flyline needs changes to these crates that upstream does not have. They used to
be git dependencies on HalFrgrd's forks; they live here so this repository can
update them itself.

| Directory | Upstream | Base commit | flyline's changes |
|---|---|---|---|
| `ratatui/` | [ratatui/ratatui](https://github.com/ratatui/ratatui) `main` | `5a28bec1` | An inline viewport positioned relative to its top row: cursor tracked in memory and never queried, resizable height, `viewport_top` for mapping mouse rows, synchronized output, autowrap off while drawing, erase instead of spaces, cells hidden behind wide glyphs skipped, no redundant cursor show/hide (HalFrgrd/ratatui `inline_viewport_improvments`, merged with upstream; see below) |
| `termina/` | [helix-editor/termina](https://github.com/helix-editor/termina) `master` | `7be7389f` (v0.4.0) | Raw device-attribute responses and no parser deadlock on DA2/DA3 or unhandled CSI (upstream PR #32), OSC 52 replies as `Paste`, the kitty base-layout key, `PlatformTerminal::with_reader` (upstream PR #31), an event buffer that keeps arrival order, and `TCSANOW` so typed-ahead input survives entering raw mode |
| `skim/` | [skim-rs/skim](https://github.com/skim-rs/skim) `master` | `2160abbd` (5.9.0) | A `tui` feature, so `default-features = false, features = ["algos_only"]` builds only the matchers, without the TUI and runtime stack (HalFrgrd/skim, 1 commit) |

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

## How ratatui differs from upstream

Upstream positions an inline viewport in absolute screen coordinates and asks
the terminal for the cursor position. flyline's fork tracks the cursor relative
to the viewport's top row and never queries it, which avoids racing bash for
the terminal's replies. Where upstream rewrote the same code, the fork's
behaviour stays:

- `set_viewport_height`, inline resize and inline `clear` keep the fork's
  versions. Upstream's `Terminal::with_cursor_position` and `insert_before`
  are not included, since both need absolute coordinates.
- `backend_mut()` does not reset the tracked cursor visibility. flyline calls it
  every frame, and re-showing the cursor each frame restarts its blink.

When pulling upstream again, expect conflicts in `ratatui-core/src/terminal/`
and in the backends' `draw` functions, and resolve them the same way.

## Change a fork

Edit the files here and commit with a `vendor(<crate>):` prefix. Changes worth
upstreaming should also be sent as PRs to the upstream repository.
