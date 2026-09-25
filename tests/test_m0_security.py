#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

# --- How to run ---
# 1. Run with the system Python (no dependencies required):
#      python3 tests/test_m0_security.py
# 2. Or run with uv:
#      uv run tests/test_m0_security.py
# ------------------

from __future__ import annotations

import json
import runpy
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Final

from m0_fixture import FixturePaths, create_valid_fixture, resign_approval_for_records


REPOSITORY_ROOT: Final = Path(__file__).resolve().parents[1]
VERIFIER: Final = REPOSITORY_ROOT / "scripts" / "verify-m0-asset.py"
sys.path.insert(0, str(REPOSITORY_ROOT / "scripts"))


def run_verifier(paths: FixturePaths) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            sys.executable,
            str(VERIFIER),
            "--candidate",
            str(paths.candidate),
            "--legal-review",
            str(paths.legal_review),
            "--approval",
            str(paths.approval),
            "--asset",
            str(paths.asset),
            "--approver-key",
            str(paths.public_key),
            "--status-out",
            str(paths.status),
        ],
        check=False,
        capture_output=True,
        text=True,
    )


def is_blocked_with_test_policy(paths: FixturePaths) -> bool:
    verifier = runpy.run_path(str(VERIFIER))
    approval = json.loads(paths.approval.read_text(encoding="utf-8"))
    arguments = verifier["parse_arguments"](
        [
            "--candidate",
            str(paths.candidate),
            "--legal-review",
            str(paths.legal_review),
            "--approval",
            str(paths.approval),
            "--asset",
            str(paths.asset),
            "--approver-key",
            str(paths.public_key),
            "--status-out",
            str(paths.status),
        ]
    )
    try:
        verifier["verify_m0"](arguments, frozenset({approval["key_id"]}))
    except (verifier["GateBlocked"], verifier["RecordError"], OSError):
        return True
    return False


def test_caller_generated_key_cannot_self_approve_via_cli() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))

        result = run_verifier(paths)

        assert result.returncode == 2, result.stderr
        assert json.loads(paths.status.read_text(encoding="utf-8")) == {"status": "BLOCKED_NO_APPROVED_ASSET"}


def test_unsigned_candidate_license_mutation_is_blocked() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        for record_path in (paths.candidate, paths.legal_review):
            record = json.loads(record_path.read_text(encoding="utf-8"))
            record["code_license"] = "MUTATED-AFTER-APPROVAL"
            record_path.write_text(json.dumps(record), encoding="utf-8")

        assert is_blocked_with_test_policy(paths)


def test_unsigned_candidate_source_mutation_is_blocked() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        candidate = json.loads(paths.candidate.read_text(encoding="utf-8"))
        candidate["source"] = "urn:sha256:" + ("0" * 64)
        paths.candidate.write_text(json.dumps(candidate), encoding="utf-8")

        assert is_blocked_with_test_policy(paths)


def test_unsigned_legal_reviewer_mutation_is_blocked() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        legal_review = json.loads(paths.legal_review.read_text(encoding="utf-8"))
        legal_review["reviewer_identity"] = "mutated-reviewer"
        paths.legal_review.write_text(json.dumps(legal_review), encoding="utf-8")

        assert is_blocked_with_test_policy(paths)


def test_unsigned_legal_review_date_mutation_is_blocked() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        legal_review = json.loads(paths.legal_review.read_text(encoding="utf-8"))
        legal_review["review_date"] = "2000-01-03"
        paths.legal_review.write_text(json.dumps(legal_review), encoding="utf-8")

        assert is_blocked_with_test_policy(paths)


def set_source_and_resign(paths: FixturePaths, source: str) -> None:
    candidate = json.loads(paths.candidate.read_text(encoding="utf-8"))
    candidate["source"] = source
    paths.candidate.write_text(json.dumps(candidate), encoding="utf-8")
    resign_approval_for_records(paths)


def test_source_digest_must_equal_asset_digest() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        set_source_and_resign(paths, "urn:sha256:" + ("0" * 64))

        assert is_blocked_with_test_policy(paths)


def test_query_digest_source_trick_is_blocked() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        candidate = json.loads(paths.candidate.read_text(encoding="utf-8"))
        set_source_and_resign(paths, f"https://example.invalid/model?sha256={candidate['sha256']}")

        assert is_blocked_with_test_policy(paths)


def test_fragment_digest_source_trick_is_blocked() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        candidate = json.loads(paths.candidate.read_text(encoding="utf-8"))
        set_source_and_resign(paths, f"https://example.invalid/model#sha256={candidate['sha256']}")

        assert is_blocked_with_test_policy(paths)


def test_path_digest_source_trick_is_blocked() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        candidate = json.loads(paths.candidate.read_text(encoding="utf-8"))
        set_source_and_resign(paths, f"https://example.invalid/{candidate['sha256']}/model")

        assert is_blocked_with_test_policy(paths)


def test_asset_symlink_is_blocked() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        target = paths.root / "asset-target.bin"
        paths.asset.rename(target)
        paths.asset.symlink_to(target)

        assert is_blocked_with_test_policy(paths)


def test_nonregular_asset_is_blocked() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        paths.asset.unlink()
        paths.asset.mkdir()

        assert is_blocked_with_test_policy(paths)


def test_public_key_symlink_is_blocked() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        target = paths.root / "public-key-target.pem"
        paths.public_key.rename(target)
        paths.public_key.symlink_to(target)

        assert is_blocked_with_test_policy(paths)


def test_public_key_snapshot_is_reused_for_id_and_signature() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        replacement_private_key = paths.root / "replacement-private-key.pem"
        replacement_public_key = paths.root / "replacement-public-key.pem"
        subprocess.run(
            ["openssl", "genpkey", "-algorithm", "ED25519", "-out", str(replacement_private_key)],
            check=True,
            capture_output=True,
        )
        subprocess.run(
            [
                "openssl",
                "pkey",
                "-in",
                str(replacement_private_key),
                "-pubout",
                "-out",
                str(replacement_public_key),
            ],
            check=True,
            capture_output=True,
        )
        replacement_key = replacement_public_key.read_bytes()
        verifier = runpy.run_path(str(VERIFIER))
        original_read = verifier["read_regular_file"]

        def replace_key_after_snapshot(path: Path) -> bytes:
            snapshot = original_read(path)
            if path == paths.public_key:
                paths.public_key.write_bytes(replacement_key)
            return snapshot

        verifier["verify_m0"].__globals__["read_regular_file"] = replace_key_after_snapshot
        approval = json.loads(paths.approval.read_text(encoding="utf-8"))
        arguments = verifier["parse_arguments"](
            [
                "--candidate",
                str(paths.candidate),
                "--legal-review",
                str(paths.legal_review),
                "--approval",
                str(paths.approval),
                "--asset",
                str(paths.asset),
                "--approver-key",
                str(paths.public_key),
                "--status-out",
                str(paths.status),
            ]
        )

        verification = verifier["verify_m0"](arguments, frozenset({approval["key_id"]}))

        assert verification.key_id == approval["key_id"]


def test_record_symlinks_are_blocked() -> None:
    for record_name in ("candidate", "legal_review", "approval"):
        with tempfile.TemporaryDirectory() as temporary_directory:
            paths = create_valid_fixture(Path(temporary_directory))
            record_path = getattr(paths, record_name)
            target = paths.root / f"{record_name}-target.json"
            record_path.rename(target)
            record_path.symlink_to(target)

            assert is_blocked_with_test_policy(paths), record_name


def test_malformed_cli_replaces_stale_approved_evidence() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        status = Path(temporary_directory) / "status.json"
        status.write_text(
            json.dumps({"status": "M0_APPROVED", "asset_sha256": "stale"}),
            encoding="utf-8",
        )

        result = subprocess.run(
            [sys.executable, str(VERIFIER), "--status-out", str(status), "--candidate"],
            check=False,
            capture_output=True,
            text=True,
        )

        assert result.returncode == 2
        assert json.loads(status.read_text(encoding="utf-8")) == {
            "status": "BLOCKED_NO_APPROVED_ASSET"
        }


if __name__ == "__main__":
    test_caller_generated_key_cannot_self_approve_via_cli()
    print("PASS test_caller_generated_key_cannot_self_approve_via_cli")
    test_unsigned_candidate_license_mutation_is_blocked()
    print("PASS test_unsigned_candidate_license_mutation_is_blocked")
    test_unsigned_candidate_source_mutation_is_blocked()
    print("PASS test_unsigned_candidate_source_mutation_is_blocked")
    test_unsigned_legal_reviewer_mutation_is_blocked()
    print("PASS test_unsigned_legal_reviewer_mutation_is_blocked")
    test_unsigned_legal_review_date_mutation_is_blocked()
    print("PASS test_unsigned_legal_review_date_mutation_is_blocked")
    test_source_digest_must_equal_asset_digest()
    print("PASS test_source_digest_must_equal_asset_digest")
    test_query_digest_source_trick_is_blocked()
    print("PASS test_query_digest_source_trick_is_blocked")
    test_fragment_digest_source_trick_is_blocked()
    print("PASS test_fragment_digest_source_trick_is_blocked")
    test_path_digest_source_trick_is_blocked()
    print("PASS test_path_digest_source_trick_is_blocked")
    test_asset_symlink_is_blocked()
    print("PASS test_asset_symlink_is_blocked")
    test_nonregular_asset_is_blocked()
    print("PASS test_nonregular_asset_is_blocked")
    test_public_key_symlink_is_blocked()
    print("PASS test_public_key_symlink_is_blocked")
    test_public_key_snapshot_is_reused_for_id_and_signature()
    print("PASS test_public_key_snapshot_is_reused_for_id_and_signature")
    test_record_symlinks_are_blocked()
    print("PASS test_record_symlinks_are_blocked")
    test_malformed_cli_replaces_stale_approved_evidence()
    print("PASS test_malformed_cli_replaces_stale_approved_evidence")
