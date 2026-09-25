#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

# --- How to run ---
# 1. Run with the system Python (no dependencies required):
#      python3 scripts/verify-m0-asset.py --help
# 2. Or run with uv:
#      uv run scripts/verify-m0-asset.py --help
# ------------------

from __future__ import annotations

import base64
import binascii
import hashlib
import json
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Final

from m0_io import read_regular_file, write_status
from m0_records import (
    Approval,
    JsonObject,
    RecordError,
    parse_json_object,
    parse_approval,
    parse_candidate,
    parse_legal_review,
)

BLOCKED_STATUS: Final = "BLOCKED_NO_APPROVED_ASSET"
APPROVED_STATUS: Final = "M0_APPROVED"
APPROVED_ASSET_ID: Final = "df-compatible-release-asset-v1"
REPOSITORY_ROOT: Final = Path(__file__).resolve().parents[1]
TRUST_POLICY_PATH: Final = REPOSITORY_ROOT / "governance" / "model-assets" / "trust-policy.json"
DEFAULT_STATUS_PATH: Final = REPOSITORY_ROOT / "docs" / "evidence" / "m0-asset-gate.json"
EXPECTED_OPTIONS: Final = ("--candidate", "--legal-review", "--approval", "--asset", "--approver-key", "--status-out")
ED25519_SPKI_PREFIX: Final = bytes.fromhex("302a300506032b6570032100")


@dataclass(frozen=True, slots=True)
class GateBlocked(Exception):
    reason: str

    def __str__(self) -> str:
        return self.reason


@dataclass(frozen=True, slots=True)
class CliArguments:
    candidate: Path
    legal_review: Path
    approval: Path
    asset: Path
    approver_key: Path
    status_out: Path


@dataclass(frozen=True, slots=True)
class ApprovedVerification:
    approval: Approval
    asset_snapshot: bytes
    asset_sha256: str
    candidate_record_sha256: str
    legal_review_record_sha256: str
    key_id: str


def parse_arguments(arguments: list[str]) -> CliArguments:
    if len(arguments) != len(EXPECTED_OPTIONS) * 2:
        raise GateBlocked("all verifier options are required")
    values: dict[str, Path] = {}
    for option_index in range(0, len(arguments), 2):
        option = arguments[option_index]
        if option not in EXPECTED_OPTIONS or option in values:
            raise GateBlocked(f"invalid option: {option}")
        values[option] = Path(arguments[option_index + 1])
    missing = set(EXPECTED_OPTIONS).difference(values)
    if missing:
        raise GateBlocked(f"missing options: {', '.join(sorted(missing))}")
    return CliArguments(
        candidate=values["--candidate"],
        legal_review=values["--legal-review"],
        approval=values["--approval"],
        asset=values["--asset"],
        approver_key=values["--approver-key"],
        status_out=values["--status-out"],
    )


def status_path_before_parse(arguments: list[str]) -> Path:
    positions = [index for index, value in enumerate(arguments) if value == "--status-out"]
    if len(positions) != 1:
        return DEFAULT_STATUS_PATH
    position = positions[0]
    if position + 1 >= len(arguments) or arguments[position + 1].startswith("--"):
        return DEFAULT_STATUS_PATH
    return Path(arguments[position + 1])


def canonical_approval(document: JsonObject) -> bytes:
    unsigned_document = {key: value for key, value in document.items() if key != "signature"}
    return canonical_document(unsigned_document)


def canonical_document(document: JsonObject) -> bytes:
    return json.dumps(document, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False).encode("utf-8")


def canonical_document_sha256(document: JsonObject) -> str:
    return hashlib.sha256(canonical_document(document)).hexdigest()


def public_key_id(public_key: bytes) -> str | None:
    result = subprocess.run(
        ["openssl", "pkey", "-pubin", "-outform", "DER"],
        input=public_key,
        check=False,
        capture_output=True,
    )
    if result.returncode != 0 or len(result.stdout) != 44 or not result.stdout.startswith(ED25519_SPKI_PREFIX):
        return None
    return f"sha256:{hashlib.sha256(result.stdout).hexdigest()}"


def verify_signature(public_key: bytes, payload: bytes, encoded_signature: str) -> bool:
    try:
        signature = base64.b64decode(encoded_signature, validate=True)
    except (binascii.Error, ValueError):
        return False
    with tempfile.TemporaryDirectory() as temporary_directory:
        payload_path = Path(temporary_directory) / "approval.json"
        signature_path = Path(temporary_directory) / "approval.sig"
        public_key_path = Path(temporary_directory) / "approver-key.pem"
        payload_path.write_bytes(payload)
        signature_path.write_bytes(signature)
        public_key_path.write_bytes(public_key)
        result = subprocess.run(
            [
                "openssl",
                "pkeyutl",
                "-verify",
                "-rawin",
                "-pubin",
                "-inkey",
                str(public_key_path),
                "-in",
                str(payload_path),
                "-sigfile",
                str(signature_path),
            ],
            check=False,
            capture_output=True,
        )
    return result.returncode == 0


def load_authorized_key_ids() -> frozenset[str]:
    policy = parse_json_object(read_regular_file(TRUST_POLICY_PATH), str(TRUST_POLICY_PATH))
    if set(policy) != {"authorized_key_ids"}:
        raise GateBlocked("trust policy fields mismatch")
    key_ids = policy["authorized_key_ids"]
    if not isinstance(key_ids, list) or not all(isinstance(key_id, str) for key_id in key_ids):
        raise GateBlocked("trust policy key IDs are invalid")
    return frozenset(key_ids)


def verify_m0(arguments: CliArguments, authorized_key_ids: frozenset[str]) -> ApprovedVerification:
    candidate_document = parse_json_object(read_regular_file(arguments.candidate), str(arguments.candidate))
    legal_review_document = parse_json_object(read_regular_file(arguments.legal_review), str(arguments.legal_review))
    candidate = parse_candidate(candidate_document, APPROVED_ASSET_ID)
    legal_review = parse_legal_review(legal_review_document)
    approval_document = parse_json_object(read_regular_file(arguments.approval), str(arguments.approval))
    approval = parse_approval(approval_document, APPROVED_ASSET_ID)
    if arguments.asset.name != f"{APPROVED_ASSET_ID}.bin":
        raise GateBlocked("asset filename is not the release asset")
    asset_snapshot = read_regular_file(arguments.asset)
    asset_sha256 = hashlib.sha256(asset_snapshot).hexdigest()
    if asset_sha256 != candidate.sha256:
        raise GateBlocked("asset SHA-256 does not match candidate provenance")
    if approval.asset_id != candidate.asset_id:
        raise GateBlocked("approval asset_id does not match candidate provenance")
    if approval.candidate_sha256 != candidate.sha256:
        raise GateBlocked("approval candidate_sha256 does not match candidate provenance")
    if approval.candidate_record_sha256 != canonical_document_sha256(candidate_document):
        raise GateBlocked("approval candidate_record_sha256 does not match candidate record")
    if approval.legal_approval_id != legal_review.legal_approval_id:
        raise GateBlocked("approval legal_approval_id does not match legal review")
    if approval.legal_review_record_sha256 != canonical_document_sha256(legal_review_document):
        raise GateBlocked("approval legal_review_record_sha256 does not match legal review record")
    if approval.conversion_terms not in {candidate.conversion_terms, legal_review.conversion_terms}:
        raise GateBlocked("conversion terms do not match provenance and legal review")
    if candidate.conversion_terms != legal_review.conversion_terms:
        raise GateBlocked("candidate and legal conversion terms do not match")
    if candidate.weight_license != legal_review.weight_license:
        raise GateBlocked("candidate and legal weight licenses do not match")
    if candidate.code_license != legal_review.code_license:
        raise GateBlocked("candidate and legal code licenses do not match")
    if approval.redistribution_terms != legal_review.redistribution_terms:
        raise GateBlocked("redistribution terms do not match legal review")
    public_key_snapshot = read_regular_file(arguments.approver_key)
    computed_key_id = public_key_id(public_key_snapshot)
    if computed_key_id is None:
        raise GateBlocked("approver public key is invalid")
    if approval.key_id != computed_key_id:
        raise GateBlocked("approval key_id does not match approver public key")
    if computed_key_id not in authorized_key_ids:
        raise GateBlocked("approver public key is not authorized")
    if not verify_signature(
        public_key_snapshot,
        canonical_approval(approval_document),
        approval.signature,
    ):
        raise GateBlocked("Ed25519 signature verification failed")
    return ApprovedVerification(
        approval=approval,
        asset_snapshot=asset_snapshot,
        asset_sha256=asset_sha256,
        candidate_record_sha256=approval.candidate_record_sha256,
        legal_review_record_sha256=approval.legal_review_record_sha256,
        key_id=computed_key_id,
    )


def approved_evidence(verification: ApprovedVerification, verified_at: str) -> JsonObject:
    return {
        "status": APPROVED_STATUS,
        "asset_sha256": verification.asset_sha256,
        "candidate_record_sha256": verification.candidate_record_sha256,
        "legal_review_record_sha256": verification.legal_review_record_sha256,
        "key_id": verification.key_id,
        "verified_at": verified_at,
    }


def main(arguments: list[str]) -> int:
    if arguments == ["--help"]:
        print("usage: verify-m0-asset.py " + " ".join(f"{option} PATH" for option in EXPECTED_OPTIONS))
        return 0
    status_path = status_path_before_parse(arguments)
    try:
        write_status(status_path, {"status": BLOCKED_STATUS})
    except OSError as error:
        print(error, file=sys.stderr)
        return 1
    try:
        parsed_arguments = parse_arguments(arguments)
    except GateBlocked as error:
        print(error, file=sys.stderr)
        return 2
    try:
        verification = verify_m0(parsed_arguments, load_authorized_key_ids())
    except (GateBlocked, RecordError, OSError, UnicodeError, json.JSONDecodeError) as error:
        print(error, file=sys.stderr)
        return 2
    write_status(
        parsed_arguments.status_out,
        approved_evidence(
            verification,
            datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        ),
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
