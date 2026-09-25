from __future__ import annotations

import json
import os
import stat
import tempfile
from pathlib import Path

from m0_records import JsonObject


def read_regular_file(path: Path) -> bytes:
    if not all(hasattr(os, flag) for flag in ("O_DIRECTORY", "O_NOFOLLOW", "O_NONBLOCK")):
        raise OSError("descriptor-relative no-follow traversal is unavailable")
    components = path.parts[1:] if path.is_absolute() else path.parts
    if not components or any(component in {"", ".", ".."} for component in components):
        raise OSError(f"{path} is not a safe file path")
    close_on_exec = getattr(os, "O_CLOEXEC", 0)
    directory_flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | close_on_exec
    directory_descriptor = os.open(path.anchor if path.is_absolute() else ".", directory_flags)
    try:
        for component in components[:-1]:
            next_descriptor = os.open(component, directory_flags, dir_fd=directory_descriptor)
            is_directory = False
            try:
                is_directory = stat.S_ISDIR(os.fstat(next_descriptor).st_mode)
                if not is_directory:
                    raise OSError(f"{path} has a non-directory parent")
            finally:
                if not is_directory:
                    os.close(next_descriptor)
            os.close(directory_descriptor)
            directory_descriptor = next_descriptor
        descriptor = os.open(
            components[-1],
            os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | close_on_exec,
            dir_fd=directory_descriptor,
        )
    finally:
        os.close(directory_descriptor)
    try:
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode):
            raise OSError(f"{path} is not a regular file")
        with os.fdopen(descriptor, "rb", closefd=False) as file_handle:
            data = file_handle.read()
        after = os.fstat(descriptor)
    finally:
        os.close(descriptor)
    before_identity = (
        before.st_dev,
        before.st_ino,
        before.st_size,
        before.st_mtime_ns,
        before.st_ctime_ns,
    )
    after_identity = (
        after.st_dev,
        after.st_ino,
        after.st_size,
        after.st_mtime_ns,
        after.st_ctime_ns,
    )
    if before_identity != after_identity or len(data) != before.st_size:
        raise OSError(f"{path} changed while being read")
    return data


def write_status(path: Path, evidence: JsonObject) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    encoded = (json.dumps(evidence, sort_keys=True) + "\n").encode("utf-8")
    descriptor, temporary_name = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.")
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "wb") as status_file:
            status_file.write(encoded)
            status_file.flush()
            os.fsync(status_file.fileno())
        os.replace(temporary_path, path)
        directory_descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory_descriptor)
        finally:
            os.close(directory_descriptor)
    except OSError:
        temporary_path.unlink(missing_ok=True)
        raise
