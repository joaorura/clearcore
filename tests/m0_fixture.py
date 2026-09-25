from __future__ import annotations

import base64
import hashlib
import json
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Final, TypeAlias


APPROVED_ASSET_ID: Final = "df-compatible-release-asset-v1"
JsonValue: TypeAlias = str | bool
JsonObject: TypeAlias = dict[str, JsonValue]


@dataclass(frozen=True, slots=True)
class FixturePaths:
    root: Path
    candidate: Path
    legal_review: Path
    approval: Path
    asset: Path
    private_key: Path
    public_key: Path
    status: Path


def canonical_approval(approval: JsonObject) -> bytes:
    unsigned = {key: value for key, value in approval.items() if key != "signature"}
    return json.dumps(
        unsigned,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
        allow_nan=False,
    ).encode("utf-8")


def canonical_record_sha256(record: JsonObject) -> str:
    canonical = json.dumps(
        record,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
        allow_nan=False,
    ).encode("utf-8")
    return hashlib.sha256(canonical).hexdigest()


def write_signed_approval(paths: FixturePaths, approval: JsonObject) -> None:
    canonical_path = paths.root / "canonical-approval.json"
    signature_path = paths.root / "signature.bin"
    canonical_path.write_bytes(canonical_approval(approval))
    subprocess.run(
        [
            "openssl",
            "pkeyutl",
            "-sign",
            "-rawin",
            "-inkey",
            str(paths.private_key),
            "-in",
            str(canonical_path),
            "-out",
            str(signature_path),
        ],
        check=True,
        capture_output=True,
    )
    approval["signature"] = base64.b64encode(signature_path.read_bytes()).decode("ascii")
    paths.approval.write_text(json.dumps(approval), encoding="utf-8")


def resign_approval_for_records(paths: FixturePaths) -> None:
    candidate: JsonObject = json.loads(paths.candidate.read_text(encoding="utf-8"))
    legal_review: JsonObject = json.loads(paths.legal_review.read_text(encoding="utf-8"))
    approval: JsonObject = json.loads(paths.approval.read_text(encoding="utf-8"))
    approval["candidate_record_sha256"] = canonical_record_sha256(candidate)
    approval["legal_review_record_sha256"] = canonical_record_sha256(legal_review)
    write_signed_approval(paths, approval)


def create_valid_fixture(root: Path) -> FixturePaths:
    paths = FixturePaths(
        root=root,
        candidate=root / "candidate.json",
        legal_review=root / "legal-review.json",
        approval=root / "approval.json",
        asset=root / f"{APPROVED_ASSET_ID}.bin",
        private_key=root / "private-key.pem",
        public_key=root / "public-key.pem",
        status=root / "status.json",
    )
    paths.asset.write_bytes(b"test-only-df-compatible-asset\n")
    candidate_sha256 = hashlib.sha256(paths.asset.read_bytes()).hexdigest()
    subprocess.run(
        ["openssl", "genpkey", "-algorithm", "ED25519", "-out", str(paths.private_key)],
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
    key_id = f"sha256:{hashlib.sha256(public_key_der).hexdigest()}"
    candidate: JsonObject = {
        "asset_id": APPROVED_ASSET_ID,
        "status": "APPROVED",
        "source": f"urn:sha256:{candidate_sha256}",
        "sha256": candidate_sha256,
        "weight_license": "TEST-ONLY",
        "code_license": "TEST-ONLY",
        "conversion_terms": "test-only conversion permitted",
    }
    legal_review: JsonObject = {
        "status": "APPROVED",
        "reviewer_identity": "test-only-reviewer",
        "review_date": "2000-01-01",
        "weight_license": "TEST-ONLY",
        "code_license": "TEST-ONLY",
        "redistribution_terms": "test-only redistribution permitted",
        "conversion_terms": "test-only conversion permitted",
        "legal_approval_id": "TEST-LEGAL-APPROVAL",
    }
    approval: JsonObject = {
        "status": "APPROVED",
        "asset_id": APPROVED_ASSET_ID,
        "approved_for_release": True,
        "candidate_sha256": candidate_sha256,
        "candidate_record_sha256": canonical_record_sha256(candidate),
        "legal_approval_id": "TEST-LEGAL-APPROVAL",
        "legal_review_record_sha256": canonical_record_sha256(legal_review),
        "approver_identity": "test-only-approver",
        "approval_date": "2000-01-02",
        "redistribution_terms": "test-only redistribution permitted",
        "conversion_terms": "test-only conversion permitted",
        "key_id": key_id,
    }
    paths.candidate.write_text(json.dumps(candidate), encoding="utf-8")
    paths.legal_review.write_text(json.dumps(legal_review), encoding="utf-8")
    write_signed_approval(paths, approval)
    return paths
