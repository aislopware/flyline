#!/usr/bin/env python3
"""Run flyline inside Neovim's `:terminal` and check that output and prompt stay put.

Neovim's terminal (libvterm) reads `CSI s` as DECSLRM, not as "save cursor",
and ignores `CSI u`. flyline wrapped its OSC 133 prompt marks in that pair, so
after moving up to the prompt-start mark the cursor stayed there: a multi-line
prompt was drawn over the previous output, and command output over the prompt
(HalFrgrd/flyline#1066; mosh has the same gap, #1070). The marks now use
DECSC / DECRC (`ESC 7` / `ESC 8`).

Skips (exit 0) when `nvim` is not installed.
"""

import argparse
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile

LUA = r"""
local out = {}
local function snap()
  local lines = vim.api.nvim_buf_get_lines(0, 0, -1, false)
  for _, line in ipairs(lines) do table.insert(out, line) end
end
vim.cmd("terminal " .. vim.env.FLYLINE_TEST_SHELL)
local chan = vim.b.terminal_job_id
vim.defer_fn(function()
  vim.fn.chansend(chan, "printf 'out1\\nout2\\nout3\\n'\r")
  vim.defer_fn(function()
    snap()
    vim.fn.writefile(out, vim.env.FLYLINE_TEST_RESULT)
    vim.cmd("qall!")
  end, 2500)
end, 3000)
"""


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bash", default="/bin/bash", type=Path)
    parser.add_argument("--flyline", required=True, type=Path)
    parser.add_argument("--nvim", default=shutil.which("nvim"))
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    if not arguments.nvim:
        print("SKIP: nvim is not installed")
        return 0

    with tempfile.TemporaryDirectory(prefix="flyline-nvim-test-") as temporary_directory:
        directory = Path(temporary_directory)
        bashrc_path = directory / "bashrc"
        bashrc_path.write_text(
            # Never let the test shell write the real ~/.bash_history.
            "HISTFILE=/dev/null\n"
            # A prompt that spans three rows, like starship's default.
            "PS1='\\n[info line]\\n> '\n"
            f"enable -f {shlex.quote(str(arguments.flyline.resolve()))} flyline\n"
            "flyline --set-delayed-startup-ms 0\n",
            encoding="utf-8",
        )
        lua_path = directory / "test.lua"
        lua_path.write_text(LUA, encoding="utf-8")
        result_path = directory / "result.txt"

        shell = shlex.join(
            [
                "env",
                "HISTFILE=/dev/null",
                str(arguments.bash),
                "--noprofile",
                "--rcfile",
                str(bashrc_path),
                "-i",
            ]
        )
        environment = os.environ | {
            "HISTFILE": "/dev/null",
            "FLYLINE_TEST_SHELL": shell,
            "FLYLINE_TEST_RESULT": str(result_path),
        }
        subprocess.run(
            [
                arguments.nvim,
                "--headless",
                "--clean",
                "-u",
                "NONE",
                "-c",
                "set columns=80 lines=24",
                "-c",
                f"luafile {lua_path}",
            ],
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=30,
            check=False,
        )
        if not result_path.exists():
            print("FAIL: nvim did not write the terminal contents", file=sys.stderr)
            return 1
        lines = [line.rstrip() for line in result_path.read_text(encoding="utf-8").splitlines()]

    while lines and not lines[-1]:
        lines.pop()
    screen = "\n".join(lines)
    expected_tail = [
        "> printf 'out1\\nout2\\nout3\\n'",
        "out1",
        "out2",
        "out3",
        "",
        "[info line]",
        ">",
    ]
    expected_head = ["", "[info line]"]
    if lines[: len(expected_head)] != expected_head or lines[-len(expected_tail) :] != expected_tail:
        print(f"FAIL: prompt and output overlap in nvim's terminal:\n{screen}", file=sys.stderr)
        return 1
    print("PASS: nvim's terminal shows the prompt, the command and its output in order")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
