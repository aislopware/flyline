#!/usr/bin/env python3
"""Verify that an idle prompt redraws only what changed.

The default cursor animates, so flyline draws a frame many times a second even
when nobody types. Each of those frames used to rewrite the whole row out to
the last column, erasing every blank cell with its own `ECH 1; CUF 1`: about
575 bytes a frame on an 80-column terminal, 12 KB a second. Some terminals
(VTE) drop the mouse selection whenever output arrives, and the traffic is
wasted over ssh. A frame now stops at the last visible cell and erases the
rest of the row with one `EL`.
"""

import argparse
import errno
import fcntl
import os
from pathlib import Path
import pty
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
PROMPT_ENABLED = b"\x1b[?2004h"
SYNC_BEGIN = b"\x1b[?2026h"
COLUMNS = 80
# "$ echo hi" with its styling, the cursor cell and the frame's bookkeeping
# take under 100 bytes. A frame that redraws the blank columns costs 500+.
MAX_BYTES_PER_FRAME = 200


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


def main() -> int:
    arguments = parse_arguments()
    with tempfile.TemporaryDirectory(prefix="flyline-idle-test-") as temporary_directory:
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
                if key not in {"TMUX", "ZELLIJ", "STY", "TERM_PROGRAM", "NO_COLOR"}
            }
            environment |= {
                "COLUMNS": str(COLUMNS),
                "LINES": "24",
                "HISTFILE": "/dev/null",
                "TERM": "xterm-256color",
            }
            os.execve(
                arguments.bash,
                [str(arguments.bash), "--noprofile", "--rcfile", str(bashrc_path), "-i"],
                environment,
            )

        fcntl.ioctl(terminal_fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, COLUMNS, 0, 0))
        output = bytearray()
        try:
            pump(terminal_fd, output, 3.0)
            if PROMPT_ENABLED not in output:
                print("FAIL: flyline never drew a prompt", file=sys.stderr)
                return 1
            for character in "echo hi":
                os.write(terminal_fd, character.encode())
                pump(terminal_fd, output, 0.05)
            pump(terminal_fd, output, 1.0)

            output.clear()
            pump(terminal_fd, output, 2.0)
        finally:
            try:
                os.kill(child_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.close(terminal_fd)
            os.waitpid(child_pid, 0)

    frames = [frame for frame in bytes(output).split(SYNC_BEGIN) if frame]
    if not frames:
        # Nothing animates, so nothing is drawn: the cheapest idle prompt there is.
        print("PASS: an idle prompt wrote nothing")
        return 0
    largest = max(len(frame) for frame in frames)
    if largest > MAX_BYTES_PER_FRAME:
        print(
            f"FAIL: an idle frame wrote {largest} bytes (limit {MAX_BYTES_PER_FRAME}): "
            f"{frames[0][:200]!r}",
            file=sys.stderr,
        )
        return 1
    print(f"PASS: {len(frames)} idle frames, at most {largest} bytes each")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
