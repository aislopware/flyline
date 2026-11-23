# flyline

[![CI](https://github.com/aislopware/flyline/actions/workflows/ci.yml/badge.svg)](https://github.com/aislopware/flyline/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/aislopware/flyline)](https://github.com/aislopware/flyline/releases)
[![Built with Ratatui](https://ratatui.rs/built-with-ratatui/badge.svg)](https://ratatui.rs/)

A Bash plugin that replaces readline with a modern line editor, written in Rust and loaded into Bash as a builtin.

![Demo](https://github.com/aislopware/flyline/releases/download/assets/demo_overview.gif)

- Autosuggestions from history and tab completion, with fuzzy search
- Syntax highlighting, tooltips, auto-closing brackets and quotes
- Mouse support: click to move the cursor, select text and suggestions, hover for tooltips
- Rich prompts: right prompt, transient prompts, async widgets, animations
- Better tab completion, plus completions synthesized from `--help` with [flycomp](https://github.com/HalFrgrd/flycomp)
- Agent mode: describe a command in plain English, or fix the last failed one
- No plugin manager, no startup scripts: one builtin in the Bash process

flyline continues [HalFrgrd/flyline](https://github.com/HalFrgrd/flyline).

## Install

```bash
source <(curl -sSfL https://github.com/aislopware/flyline/releases/latest/download/install.sh)
```

This downloads the library and adds it to your `.bashrc`, no `sudo` needed. On macOS (which needs Homebrew Bash) you can also install with Homebrew:

```bash
brew install bash aislopware/tap/flyline
echo 'enable -f "$(brew --prefix)/lib/bash/flyline" flyline' >> ~/.bashrc
```

Nix, manual downloads and building from source: see the [guide](docs/guide.md#installation-details).

| Platform | Requirement |
|---|---|
| Linux x86_64 / aarch64 | glibc 2.34+ (RHEL 9, Ubuntu 22.04, Debian 13 and newer) or musl |
| macOS | Apple silicon, macOS 11+, Homebrew Bash |
| Android | aarch64, Termux |

flyline needs Bash 4.4 or newer and is tested on 5.1, 5.2 and 5.3. Other platforms can [build from source](docs/guide.md#build-from-source).

## Get started

```bash
flyline run-tutorial          # interactive tour of keys and mouse
flyline <Tab>                 # browse every setting with tab completion
flyline mouse --mode disabled # if you don't want mouse capture
flyline upgrade               # update to the latest release
enable -d flyline             # turn it off for this shell
```

Settings are plain commands, so put the ones you like in `.bashrc`. A few examples:

```bash
RPS1='\t'                                        # right prompt with the time
flyline key bind Ctrl+b 'always=runBashCommand(my_fn)'
flyline set-agent-mode --help                    # set up agent mode (see examples/agent_mode.sh)
```

The [guide](docs/guide.md) covers prompts and widgets, keybindings, history, completion, agent mode and terminal-specific notes. See also the [examples](examples/).

## Terminal tips

flyline uses modern terminal features, so a full-featured terminal such as Ghostty or Kitty works best. In Ghostty, set `cursor-click-to-move = false`. See the [terminal notes](docs/guide.md#terminal-emulator-notes) for Kitty, VS Code, macOS and GNU Screen.

## License

The source code is [MIT](LICENSE-MIT). The Bash lexer in [`src/grammar/lexer.rs`](src/grammar/lexer.rs) derives from [flash](https://github.com/raphamorim/flash) (GPL-3.0). Because the plugin links against GNU Bash, compiled binaries and combined works are covered by the [GPLv3](LICENSE-GPLv3).
