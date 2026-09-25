from __future__ import annotations

import json
import re
from dataclasses import dataclass
from datetime import date
from typing import Final, TypeAlias

JsonValue: TypeAlias = str | int | float | bool | None | list["JsonValue"] | dict[str, "JsonValue"]
JsonObject: TypeAlias = dict[str, JsonValue]

SHA256_PATTERN: Final = re.compile(r"[0-9a-f]{64}")
CANDIDATE_FIELDS: Final = frozenset(
    {"status", "asset_id", "source", "sha256", "weight_license", "code_license", "conversion_terms"}
)
LEGAL_FIELDS: Final = frozenset(
    {
        "status",
        "reviewer_identity",
        "review_date",
        "redistribution_terms",
        "conversion_terms",
        "legal_approval_id",
        "weight_license",
        "code_license",
    }
)
APPROVAL_FIELDS: Final = frozenset(
    {
        "status",
        "asset_id",
        "approved_for_release",
        "candidate_sha256",
        "candidate_record_sha256",
        "legal_approval_id",
        "legal_review_record_sha256",
        "approver_identity",
        "approval_date",
        "redistribution_terms",
        "conversion_terms",
        "key_id",
        "signature",
    }
)


class RecordError(ValueError):
    pass


@dataclass(frozen=True, slots=True)
class Candidate:
    asset_id: str
    source: str
    sha256: str
    weight_license: str
    code_license: str
    conversion_terms: str


@dataclass(frozen=True, slots=True)
class LegalReview:
    reviewer_identity: str
    review_date: str
    redistribution_terms: str
    conversion_terms: str
    legal_approval_id: str
    weight_license: str
    code_license: str


@dataclass(frozen=True, slots=True)
class Approval:
    asset_id: str
    candidate_sha256: str
    candidate_record_sha256: str
    legal_approval_id: str
    legal_review_record_sha256: str
    approver_identity: str
    approval_date: str
    redistribution_terms: str
    conversion_terms: str
    key_id: str
    signature: str


def reject_duplicate_keys(pairs: list[tuple[str, JsonValue]]) -> JsonObject:
    document: JsonObject = {}
    for key, value in pairs:
        if key in document:
            raise RecordError(f"duplicate JSON field: {key}")
        document[key] = value
    return document


def reject_nonfinite(value: str) -> None:
    raise RecordError(f"non-finite JSON value: {value}")


def parse_json_object(data: bytes, source_name: str) -> JsonObject:
    loaded = json.loads(
        data.decode("utf-8"),
        object_pairs_hook=reject_duplicate_keys,
        parse_constant=reject_nonfinite,
    )
    if not isinstance(loaded, dict):
        raise RecordError(f"{source_name} must contain a JSON object")
    return loaded


def require_exact_fields(document: JsonObject, expected: frozenset[str], record_name: str) -> None:
    actual = set(document)
    if actual != expected:
        missing = sorted(expected - actual)
        unexpected = sorted(actual - expected)
        raise RecordError(f"{record_name} fields mismatch; missing={missing}, unexpected={unexpected}")


def required_string(document: JsonObject, field: str) -> str:
    value = document[field]
    if not isinstance(value, str) or not value.strip():
        raise RecordError(f"{field} must be a non-empty string")
    return value


def required_date(document: JsonObject, field: str) -> str:
    value = required_string(document, field)
    try:
        parsed = date.fromisoformat(value)
    except ValueError:
        raise RecordError(f"{field} must be an ISO 8601 date") from None
    if parsed.isoformat() != value:
        raise RecordError(f"{field} must use YYYY-MM-DD")
    return value


def require_approved(document: JsonObject, record_name: str) -> None:
    if document["status"] != "APPROVED":
        raise RecordError(f"{record_name} status must be APPROVED")


def is_immutable_source(source: str, asset_sha256: str) -> bool:
    return source == f"urn:sha256:{asset_sha256}"


def parse_candidate(document: JsonObject, expected_asset_id: str) -> Candidate:
    require_exact_fields(document, CANDIDATE_FIELDS, "candidate")
    require_approved(document, "candidate")
    candidate = Candidate(
        asset_id=required_string(document, "asset_id"),
        source=required_string(document, "source"),
        sha256=required_string(document, "sha256"),
        weight_license=required_string(document, "weight_license"),
        code_license=required_string(document, "code_license"),
        conversion_terms=required_string(document, "conversion_terms"),
    )
    if candidate.asset_id != expected_asset_id:
        raise RecordError("candidate asset_id is not the release asset")
    if SHA256_PATTERN.fullmatch(candidate.sha256) is None:
        raise RecordError("candidate sha256 must be 64 lowercase hexadecimal characters")
    if not is_immutable_source(candidate.source, candidate.sha256):
        raise RecordError("candidate source must be the exact asset SHA-256 URN")
    return candidate


def parse_legal_review(document: JsonObject) -> LegalReview:
    require_exact_fields(document, LEGAL_FIELDS, "legal review")
    require_approved(document, "legal review")
    return LegalReview(
        reviewer_identity=required_string(document, "reviewer_identity"),
        review_date=required_date(document, "review_date"),
        redistribution_terms=required_string(document, "redistribution_terms"),
        conversion_terms=required_string(document, "conversion_terms"),
        legal_approval_id=required_string(document, "legal_approval_id"),
        weight_license=required_string(document, "weight_license"),
        code_license=required_string(document, "code_license"),
    )


def parse_approval(document: JsonObject, expected_asset_id: str) -> Approval:
    require_exact_fields(document, APPROVAL_FIELDS, "approval")
    require_approved(document, "approval")
    if document["approved_for_release"] is not True:
        raise RecordError("approved_for_release must be true")
    approval = Approval(
        asset_id=required_string(document, "asset_id"),
        candidate_sha256=required_string(document, "candidate_sha256"),
        candidate_record_sha256=required_string(document, "candidate_record_sha256"),
        legal_approval_id=required_string(document, "legal_approval_id"),
        legal_review_record_sha256=required_string(document, "legal_review_record_sha256"),
        approver_identity=required_string(document, "approver_identity"),
        approval_date=required_date(document, "approval_date"),
        redistribution_terms=required_string(document, "redistribution_terms"),
        conversion_terms=required_string(document, "conversion_terms"),
        key_id=required_string(document, "key_id"),
        signature=required_string(document, "signature"),
    )
    if approval.asset_id != expected_asset_id:
        raise RecordError("approval asset_id is not the release asset")
    if SHA256_PATTERN.fullmatch(approval.candidate_record_sha256) is None:
        raise RecordError("candidate_record_sha256 must be 64 lowercase hexadecimal characters")
    if SHA256_PATTERN.fullmatch(approval.legal_review_record_sha256) is None:
        raise RecordError("legal_review_record_sha256 must be 64 lowercase hexadecimal characters")
    return approval
