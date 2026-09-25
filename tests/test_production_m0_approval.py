#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

# --- How to run ---
# 1. Run with the system Python (no dependencies required):
#      PYTHONDONTWRITEBYTECODE=1 python3 tests/test_production_m0_approval.py
# ------------------

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Final


ASSET_ID: Final = "df-compatible-release-asset-v1"
ASSET_SHA256: Final = "c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616"
PRODUCTION_KEY_ID: Final = "sha256:cfa7e10c021031f5481775cf32093d41e5aaaad454653f3ed0022769cbbfd3f2"
REPOSITORY_ROOT: Final = Path(__file__).resolve().parents[1]
DOSSIER: Final = REPOSITORY_ROOT / "governance" / "model-assets" / ASSET_ID
VERIFIER: Final = REPOSITORY_ROOT / "scripts" / "verify-m0-asset.py"


def test_production_dossier_is_authorized_and_approved() -> None:
    candidate = DOSSIER / "candidate-provenance.json"
    legal_review = DOSSIER / "legal-review.json"
    approval = DOSSIER / "approval-manifest.json"
    public_key = DOSSIER / "approver-public-key.pem"
    asset = REPOSITORY_ROOT / "vendor" / "approved" / f"{ASSET_ID}.bin"
    trust_policy = json.loads(
        (REPOSITORY_ROOT / "governance" / "model-assets" / "trust-policy.json").read_text(
            encoding="utf-8"
        )
    )

    assert trust_policy == {"authorized_key_ids": [PRODUCTION_KEY_ID]}
    assert json.loads(candidate.read_text(encoding="utf-8")) == {
        "status": "APPROVED",
        "asset_id": ASSET_ID,
        "source": f"urn:sha256:{ASSET_SHA256}",
        "sha256": ASSET_SHA256,
        "weight_license": "MIT OR Apache-2.0",
        "code_license": "MIT OR Apache-2.0",
        "conversion_terms": "Redistributed byte-for-byte as the upstream ONNX archive under an opaque release filename; no model conversion or quantization has occurred.",
    }
    assert json.loads(legal_review.read_text(encoding="utf-8"))["reviewer_identity"] == "João Messias Lima Pereira"
    assert json.loads(approval.read_text(encoding="utf-8"))["approver_identity"] == "João Messias Lima Pereira"

    with tempfile.TemporaryDirectory() as temporary_directory:
        status_out = Path(temporary_directory) / "m0-status.json"
        result = subprocess.run(
            [
                sys.executable,
                str(VERIFIER),
                "--candidate",
                str(candidate),
                "--legal-review",
                str(legal_review),
                "--approval",
                str(approval),
                "--asset",
                str(asset),
                "--approver-key",
                str(public_key),
                "--status-out",
                str(status_out),
            ],
            check=False,
            capture_output=True,
            text=True,
        )

        assert result.returncode == 0, result.stderr
        assert json.loads(status_out.read_text(encoding="utf-8"))["status"] == "M0_APPROVED"


if __name__ == "__main__":
    test_production_dossier_is_authorized_and_approved()
    print("PASS test_production_dossier_is_authorized_and_approved")
