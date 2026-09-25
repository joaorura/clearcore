#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

# --- How to run ---
# 1. Run with the system Python (no dependencies required):
#      python3 tests/test_verify_m0_asset.py
# 2. Or run with uv:
#      uv run tests/test_verify_m0_asset.py
# ------------------

from __future__ import annotations

import io
import json
import hashlib
import runpy
import subprocess
import sys
import tempfile
from collections.abc import Callable
from contextlib import redirect_stderr
from pathlib import Path
from typing import Final

from m0_fixture import (
    APPROVED_ASSET_ID,
    FixturePaths,
    JsonObject,
    JsonValue,
    create_valid_fixture,
    write_signed_approval,
)


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


def verifier_arguments(paths: FixturePaths) -> list[str]:
    return [
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


def run_verifier_with_test_policy(paths: FixturePaths, authorized_key_id: str) -> tuple[int, str]:
    verifier = runpy.run_path(str(VERIFIER))
    verifier["main"].__globals__["load_authorized_key_ids"] = lambda: frozenset({authorized_key_id})
    stderr = io.StringIO()
    with redirect_stderr(stderr):
        return verifier["main"](verifier_arguments(paths)), stderr.getvalue()


def assert_status(paths: FixturePaths, expected_status: str) -> None:
    evidence = json.loads(paths.status.read_text(encoding="utf-8"))
    assert evidence["status"] == expected_status


def rewrite_json_field(path: Path, field: str, value: JsonValue) -> None:
    document = json.loads(path.read_text(encoding="utf-8"))
    document[field] = value
    path.write_text(json.dumps(document), encoding="utf-8")


def assert_mutation_is_blocked(
    mutation: Callable[[FixturePaths], None], expected_error: str | None = None
) -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        approval = json.loads(paths.approval.read_text(encoding="utf-8"))
        key_id = approval["key_id"]
        assert isinstance(key_id, str)
        mutation(paths)

        returncode, stderr = run_verifier_with_test_policy(paths, key_id)

        assert returncode == 2, stderr
        assert_status(paths, "BLOCKED_NO_APPROVED_ASSET")
        if expected_error is not None:
            assert expected_error in stderr


def test_incomplete_fixture_is_blocked() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        root = Path(temporary_directory)
        paths = FixturePaths(
            root=root,
            candidate=root / "candidate.json",
            legal_review=root / "legal-review.json",
            approval=root / "approval.json",
            asset=root / f"{APPROVED_ASSET_ID}.bin",
            private_key=root / "unused-private-key.pem",
            public_key=root / "public-key.pem",
            status=root / "status.json",
        )
        paths.candidate.write_text("{}\n", encoding="utf-8")
        paths.legal_review.write_text("{}\n", encoding="utf-8")
        paths.approval.write_text("{}\n", encoding="utf-8")
        paths.public_key.write_text("", encoding="utf-8")

        result = run_verifier(paths)

        assert result.returncode == 2, result.stderr
        assert_status(paths, "BLOCKED_NO_APPROVED_ASSET")


def test_complete_signed_fixture_is_approved() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        verifier = runpy.run_path(str(VERIFIER))
        approval = json.loads(paths.approval.read_text(encoding="utf-8"))
        arguments = verifier["parse_arguments"](verifier_arguments(paths))

        result = verifier["verify_m0"](arguments, frozenset({approval["key_id"]}))

        assert result.approval.asset_id == APPROVED_ASSET_ID


def test_complete_signed_fixture_is_approved_under_fixture_test_policy() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        approval = json.loads(paths.approval.read_text(encoding="utf-8"))
        key_id = approval["key_id"]
        assert isinstance(key_id, str)

        returncode, stderr = run_verifier_with_test_policy(paths, key_id)

        assert returncode == 0, stderr
        assert_status(paths, "M0_APPROVED")


def test_pending_status_is_blocked() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
        approval = json.loads(paths.approval.read_text(encoding="utf-8"))
        approval["status"] = "PENDING"
        write_signed_approval(paths, approval)

        result = run_verifier(paths)

        assert result.returncode == 2, result.stderr
        assert_status(paths, "BLOCKED_NO_APPROVED_ASSET")


def test_pending_candidate_or_legal_review_is_blocked() -> None:
    for record_name in ("candidate", "legal_review"):
        with tempfile.TemporaryDirectory() as temporary_directory:
            paths = create_valid_fixture(Path(temporary_directory))
            record_path = getattr(paths, record_name)
            record = json.loads(record_path.read_text(encoding="utf-8"))
            record["status"] = "PENDING"
            record_path.write_text(json.dumps(record), encoding="utf-8")

            result = run_verifier(paths)

            assert result.returncode == 2, f"{record_name}: {result.stderr}"
            assert_status(paths, "BLOCKED_NO_APPROVED_ASSET")


def test_empty_required_field_is_blocked() -> None:
    assert_mutation_is_blocked(
        lambda paths: rewrite_json_field(paths.candidate, "source", ""),
        "source must be a non-empty string",
    )


def test_missing_record_is_blocked() -> None:
    assert_mutation_is_blocked(lambda paths: paths.legal_review.unlink())


def test_malformed_json_is_blocked() -> None:
    assert_mutation_is_blocked(lambda paths: paths.approval.write_text("{", encoding="utf-8"))


def test_hash_mismatch_is_blocked() -> None:
    assert_mutation_is_blocked(lambda paths: paths.asset.write_bytes(b"tampered"))


def test_alternative_asset_id_is_blocked() -> None:
    assert_mutation_is_blocked(
        lambda paths: rewrite_json_field(paths.candidate, "asset_id", "alternative-asset")
    )


def test_legal_approval_identity_mismatch_is_blocked() -> None:
    assert_mutation_is_blocked(
        lambda paths: rewrite_json_field(paths.legal_review, "legal_approval_id", "OTHER-LEGAL-ID")
    )


def test_invalid_signature_is_blocked() -> None:
    assert_mutation_is_blocked(
        lambda paths: rewrite_json_field(paths.approval, "signature", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==")
    )


def remove_approval_signature(paths: FixturePaths) -> None:
    approval = json.loads(paths.approval.read_text(encoding="utf-8"))
    del approval["signature"]
    paths.approval.write_text(json.dumps(approval), encoding="utf-8")


def test_unsigned_approval_is_blocked() -> None:
    assert_mutation_is_blocked(remove_approval_signature)


def set_release_approval_false(paths: FixturePaths) -> None:
    approval: JsonObject = json.loads(paths.approval.read_text(encoding="utf-8"))
    approval["approved_for_release"] = False
    write_signed_approval(paths, approval)


def test_release_approval_false_is_blocked() -> None:
    assert_mutation_is_blocked(set_release_approval_false)


def set_wrong_candidate_sha256(paths: FixturePaths) -> None:
    approval: JsonObject = json.loads(paths.approval.read_text(encoding="utf-8"))
    approval["candidate_sha256"] = "0" * 64
    write_signed_approval(paths, approval)


def test_candidate_sha256_identity_mismatch_is_blocked() -> None:
    assert_mutation_is_blocked(set_wrong_candidate_sha256)


def test_conversion_terms_mismatch_is_blocked() -> None:
    assert_mutation_is_blocked(
        lambda paths: rewrite_json_field(paths.legal_review, "conversion_terms", "different terms")
    )


def set_wrong_key_id(paths: FixturePaths) -> None:
    approval: JsonObject = json.loads(paths.approval.read_text(encoding="utf-8"))
    approval["key_id"] = "sha256:" + ("0" * 64)
    write_signed_approval(paths, approval)


def test_key_identity_mismatch_is_blocked() -> None:
    assert_mutation_is_blocked(set_wrong_key_id)


def use_key_algorithm(paths: FixturePaths, algorithm: str) -> None:
    subprocess.run(
        ["openssl", "genpkey", "-algorithm", algorithm, "-out", str(paths.private_key)],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["openssl", "pkey", "-in", str(paths.private_key), "-pubout", "-out", str(paths.public_key)],
        check=True,
        capture_output=True,
    )
    public_key_der = subprocess.run(
        ["openssl", "pkey", "-pubin", "-in", str(paths.public_key), "-outform", "DER"],
        check=True,
        capture_output=True,
    ).stdout
    approval: JsonObject = json.loads(paths.approval.read_text(encoding="utf-8"))
    approval["key_id"] = f"sha256:{hashlib.sha256(public_key_der).hexdigest()}"
    write_signed_approval(paths, approval)


def test_non_ed25519_key_is_blocked() -> None:
    assert_mutation_is_blocked(lambda paths: use_key_algorithm(paths, "ED448"))


def set_rejected_approval_status(paths: FixturePaths) -> None:
    approval: JsonObject = json.loads(paths.approval.read_text(encoding="utf-8"))
    approval["status"] = "REJECTED"
    write_signed_approval(paths, approval)


def test_nonapproved_status_is_blocked() -> None:
    assert_mutation_is_blocked(set_rejected_approval_status)


def test_extra_record_field_is_blocked() -> None:
    assert_mutation_is_blocked(
        lambda paths: rewrite_json_field(paths.candidate, "unreviewed_field", "unexpected")
    )


def test_mutable_source_is_blocked() -> None:
    assert_mutation_is_blocked(
        lambda paths: rewrite_json_field(paths.candidate, "source", "https://example.invalid/latest.bin")
    )


def remove_legal_license_review(paths: FixturePaths) -> None:
    legal_review = json.loads(paths.legal_review.read_text(encoding="utf-8"))
    del legal_review["weight_license"]
    del legal_review["code_license"]
    paths.legal_review.write_text(json.dumps(legal_review), encoding="utf-8")


def test_missing_legal_license_review_is_blocked() -> None:
    assert_mutation_is_blocked(remove_legal_license_review)


def add_nonfinite_approval_value(paths: FixturePaths) -> None:
    approval = paths.approval.read_text(encoding="utf-8").rstrip("}")
    paths.approval.write_text(approval + ',"nonfinite":NaN}', encoding="utf-8")


def test_nonfinite_json_is_blocked_with_exit_two() -> None:
    assert_mutation_is_blocked(add_nonfinite_approval_value)


def main() -> int:
    tests = (
        test_incomplete_fixture_is_blocked,
        test_complete_signed_fixture_is_approved,
        test_complete_signed_fixture_is_approved_under_fixture_test_policy,
        test_pending_status_is_blocked,
        test_pending_candidate_or_legal_review_is_blocked,
        test_empty_required_field_is_blocked,
        test_missing_record_is_blocked,
        test_malformed_json_is_blocked,
        test_hash_mismatch_is_blocked,
        test_alternative_asset_id_is_blocked,
        test_legal_approval_identity_mismatch_is_blocked,
        test_invalid_signature_is_blocked,
        test_unsigned_approval_is_blocked,
        test_release_approval_false_is_blocked,
        test_candidate_sha256_identity_mismatch_is_blocked,
        test_conversion_terms_mismatch_is_blocked,
        test_key_identity_mismatch_is_blocked,
        test_non_ed25519_key_is_blocked,
        test_nonapproved_status_is_blocked,
        test_extra_record_field_is_blocked,
        test_mutable_source_is_blocked,
        test_missing_legal_license_review_is_blocked,
        test_nonfinite_json_is_blocked_with_exit_two,
    )
    for test in tests:
        test()
        print(f"PASS {test.__name__}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
