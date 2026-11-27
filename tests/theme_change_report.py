#!/usr/bin/env python3
"""Verify that flyline follows a terminal's dark/light switch (DEC mode 2031).

flyline enables mode 2031 while it edits a line. A terminal that supports it
sends `CSI ? 997 ; 1|2 n` when its theme changes; flyline then asks for the new
foreground and background (OSC 10/11) so its palette follows. The mode is
switched off again before a command runs, so the report never reaches the
command's input.
"""

import argparse
import errno
import fcntl
import os
from pathlib import Path
import pty
import re
import select
import shlex
import signal
import struct
import sys
import tempfile
import termios
import time

DEVICE_ATTRIBUTES_QUERIES = (b"\x1b[>c", b"\x1b[c")
DEVICE_ATTRIBUTES_RESPONSE = b"\x1b[>1;1;0c"
CURSOR_POSITION_QUERY = b"\x1b[6n"
CURSOR_POSITION_RESPONSE = b"\x1b[1;1R"
THEME_MODE_ON = b"\x1b[?2031h"
THEME_MODE_OFF = b"\x1b[?2031l"
LIGHT_THEME_REPORT = b"\x1b[?997;2n"
BACKGROUND_QUERY = b"\x1b]11;?"
LIGHT_COLOURS = b"\x1b]10;rgb:0000/0000/0000\x1b\\\x1b]11;rgb:ffff/ffff/ffff\x1b\\"
SGR = re.compile(rb"\x1b\[[0-9;:]*m")


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bash", default="/bin/bash", type=Path)
    parser.add_argument("--flyline", required=True, type=Path)
    return parser.parse_args()


def pump(terminal_fd: int, output: bytearray, seconds: float) -> None:
    """Read output for a while, answering device-attribute and cursor queries."""
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        readable, _, _ = select.select([terminal_fd], [], [], 0.02)
        if not readable:
            continue
        try:
            data = os.read(terminal_fd, 65_536)
        except OSError as error:
            if error.errno != errno.EIO:
                raise
            return
        output.extend(data)
        if CURSOR_POSITION_QUERY in data:
            os.write(terminal_fd, CURSOR_POSITION_RESPONSE)
        if any(query in data for query in DEVICE_ATTRIBUTES_QUERIES):
            os.write(terminal_fd, DEVICE_ATTRIBUTES_RESPONSE)


def style_before_command_word(output: bytes) -> bytes | None:
    """The last SGR sequence written before the last `echo` drawn."""
    end = output.rfind(b"echo")
    if end < 0:
        return None
    styles = SGR.findall(output, 0, end)
    return styles[-1] if styles else b""


def main() -> int:
    arguments = parse_arguments()
    with tempfile.TemporaryDirectory(prefix="flyline-theme-test-") as temporary_directory:
        bashrc_path = Path(temporary_directory) / "bashrc"
        bashrc_path.write_text(
            # Never let the test shell write the real ~/.bash_history.
            "HISTFILE=/dev/null\n"
            "PS1='$ '\n"
            f"enable -f {shlex.quote(str(arguments.flyline))} flyline\n"
            "flyline --set-delayed-startup-ms 0\n",
            encoding="utf-8",
        )

        child_pid, terminal_fd = pty.fork()
        if child_pid == 0:
            environment = {
                key: value
                for key, value in os.environ.items()
                if key not in {"TMUX", "ZELLIJ", "STY", "TERM_PROGRAM"}
            }
            environment |= {"HISTFILE": "/dev/null", "TERM": "xterm-256color"}
            os.execve(
                arguments.bash,
                [str(arguments.bash), "--noprofile", "--rcfile", str(bashrc_path), "-i"],
                environment,
            )

        fcntl.ioctl(terminal_fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        output = bytearray()
        try:
            pump(terminal_fd, output, 3.0)
            if THEME_MODE_ON not in output:
                print("FAIL: flyline never enabled theme reports (mode 2031)", file=sys.stderr)
                return 1

            for character in "echo hi":
                os.write(terminal_fd, character.encode())
                pump(terminal_fd, output, 0.05)
            pump(terminal_fd, output, 0.5)
            dark_style = style_before_command_word(bytes(output))

            output.clear()
            os.write(terminal_fd, LIGHT_THEME_REPORT)
            pump(terminal_fd, output, 1.0)
            if BACKGROUND_QUERY not in output:
                print(
                    f"FAIL: a theme report did not re-query the colours: {bytes(output)[:200]!r}",
                    file=sys.stderr,
                )
                return 1
            output.clear()
            os.write(terminal_fd, LIGHT_COLOURS)
            pump(terminal_fd, output, 1.0)
            light_style = style_before_command_word(bytes(output))
            if light_style is None or light_style == dark_style:
                print(
                    "FAIL: the command line kept its old colours after the theme switch "
                    f"(before {dark_style!r}, after {light_style!r})",
                    file=sys.stderr,
                )
                return 1

            # Clear the line, then switch the theme right before Enter: the colour
            # replies arrive after the command started and must not become its input.
            os.write(terminal_fd, b"\x15")
            pump(terminal_fd, output, 0.3)
            os.write(terminal_fd, b'read -t 1 -r -N 100 x; echo "GOT[${#x}]"')
            pump(terminal_fd, output, 0.5)
            output.clear()
            os.write(terminal_fd, LIGHT_THEME_REPORT + b"\r")
            time.sleep(0.05)
            os.write(terminal_fd, LIGHT_COLOURS)
            pump(terminal_fd, output, 2.0)
            if THEME_MODE_OFF not in output:
                print("FAIL: mode 2031 stayed on while a command ran", file=sys.stderr)
                return 1
            if b"GOT[0]" not in output:
                print(
                    f"FAIL: colour replies leaked into the command's input: {bytes(output)[-300:]!r}",
                    file=sys.stderr,
                )
                return 1
        finally:
            try:
                os.kill(child_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.close(terminal_fd)
            os.waitpid(child_pid, 0)

    print(
        "PASS: a theme switch restyles the command line, and its replies never reach a command"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
