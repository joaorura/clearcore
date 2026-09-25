#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

# --- How to run ---
# 1. Run with the system Python (no dependencies required):
#      python3 tests/test_m0_status.py
# 2. Or run with uv:
#      uv run tests/test_m0_status.py
# ------------------

from __future__ import annotations

import json
import runpy
import sys
import tempfile
from pathlib import Path
from typing import Final

from m0_fixture import APPROVED_ASSET_ID, create_valid_fixture


REPOSITORY_ROOT: Final = Path(__file__).resolve().parents[1]
VERIFIER: Final = REPOSITORY_ROOT / "scripts" / "verify-m0-asset.py"
sys.path.insert(0, str(REPOSITORY_ROOT / "scripts"))


def test_approved_evidence_is_complete_and_atomic() -> None:
    with tempfile.TemporaryDirectory() as temporary_directory:
        paths = create_valid_fixture(Path(temporary_directory))
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
        verification = verifier["verify_m0"](arguments, frozenset({approval["key_id"]}))
        evidence = verifier["approved_evidence"](verification, "2000-01-03T00:00:00Z")

        verifier["write_status"](paths.status, evidence)

        assert json.loads(paths.status.read_text(encoding="utf-8")) == {
            "status": "M0_APPROVED",
            "asset_sha256": approval["candidate_sha256"],
            "candidate_record_sha256": approval["candidate_record_sha256"],
            "legal_review_record_sha256": approval["legal_review_record_sha256"],
            "key_id": approval["key_id"],
            "verified_at": "2000-01-03T00:00:00Z",
        }
        assert verification.approval.asset_id == APPROVED_ASSET_ID
        assert list(paths.root.glob(f".{paths.status.name}.*")) == []


def test_missing_or_ambiguous_status_output_uses_repository_evidence_path() -> None:
    verifier = runpy.run_path(str(VERIFIER))

    assert verifier["status_path_before_parse"]([]) == verifier["DEFAULT_STATUS_PATH"]
    assert verifier["status_path_before_parse"](
        ["--status-out", "first.json", "--status-out", "second.json"]
    ) == verifier["DEFAULT_STATUS_PATH"]


if __name__ == "__main__":
    test_approved_evidence_is_complete_and_atomic()
    print("PASS test_approved_evidence_is_complete_and_atomic")
    test_missing_or_ambiguous_status_output_uses_repository_evidence_path()
    print("PASS test_missing_or_ambiguous_status_output_uses_repository_evidence_path")
