#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import os
import socket
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Final


REPOSITORY_ROOT: Final = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPOSITORY_ROOT / "scripts"))

from m0_io import read_regular_file


def test_read_regular_file_rejects_symlinked_parent_directory() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        root = Path(temporary_directory)
        target_directory = root / "target"
        target_directory.mkdir()
        (target_directory / "payload").write_bytes(b"payload")
        alias = root / "alias"
        alias.symlink_to(target_directory, target_is_directory=True)
        original_directory = Path.cwd()
        try:
            os.chdir(root)
            for path in (Path("alias") / "payload", alias / "payload"):
                try:
                    read_regular_file(path)
                except OSError:
                    continue
                raise AssertionError(f"accepted symlinked parent directory: {path}")
        finally:
            os.chdir(original_directory)


def test_read_regular_file_rejects_fifo_without_blocking() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        fifo_path = Path(temporary_directory) / "payload"
        os.mkfifo(fifo_path)
        child = (
            "from pathlib import Path\n"
            "import sys\n"
            f"sys.path.insert(0, {str(REPOSITORY_ROOT / 'scripts')!r})\n"
            "from m0_io import read_regular_file\n"
            "try:\n"
            f"    read_regular_file(Path({str(fifo_path)!r}))\n"
            "except OSError:\n"
            "    pass\n"
            "else:\n"
            "    raise AssertionError('accepted FIFO')\n"
        )
        try:
            result = subprocess.run(
                [sys.executable, "-B", "-c", child],
                check=False,
                capture_output=True,
                text=True,
                timeout=1,
            )
        except subprocess.TimeoutExpired as error:
            raise AssertionError("FIFO read blocked before rejection") from error
        assert result.returncode == 0, result.stderr


def test_read_regular_file_rejects_unix_socket() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        socket_path = Path(temporary_directory) / "payload"
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
            listener.bind(str(socket_path))
            try:
                read_regular_file(socket_path)
            except OSError:
                return
            raise AssertionError("accepted Unix socket")


if __name__ == "__main__":
    test_read_regular_file_rejects_symlinked_parent_directory()
    test_read_regular_file_rejects_fifo_without_blocking()
    test_read_regular_file_rejects_unix_socket()
    print("PASS test_read_regular_file_rejects_symlinked_parent_directory")
