use std::collections::BTreeSet;

use serde_json::{Map, Value};

const CANDIDATE_FIELDS: [&str; 7] = [
    "asset_id",
    "code_license",
    "conversion_terms",
    "sha256",
    "source",
    "status",
    "weight_license",
];
const LEGAL_FIELDS: [&str; 8] = [
    "code_license",
    "conversion_terms",
    "legal_approval_id",
    "redistribution_terms",
    "review_date",
    "reviewer_identity",
    "status",
    "weight_license",
];
const APPROVAL_FIELDS: [&str; 13] = [
    "approval_date",
    "approved_for_release",
    "approver_identity",
    "asset_id",
    "candidate_record_sha256",
    "candidate_sha256",
    "conversion_terms",
    "key_id",
    "legal_approval_id",
    "legal_review_record_sha256",
    "redistribution_terms",
    "signature",
    "status",
];

pub fn validate(candidate: &Value, legal: &Value, approval: &Value) -> Result<(), String> {
    let candidate = object(candidate, "candidate")?;
    let legal = object(legal, "legal review")?;
    let approval = object(approval, "approval")?;
    exact_fields(candidate, &CANDIDATE_FIELDS, "candidate")?;
    exact_fields(legal, &LEGAL_FIELDS, "legal review")?;
    exact_fields(approval, &APPROVAL_FIELDS, "approval")?;
    approved(candidate, "candidate")?;
    approved(legal, "legal review")?;
    approved(approval, "approval")?;
    if approval
        .get("approved_for_release")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err("approved_for_release must be true".to_owned());
    }
    for (record, fields) in [
        (
            candidate,
            &[
                "asset_id",
                "source",
                "sha256",
                "weight_license",
                "code_license",
                "conversion_terms",
            ][..],
        ),
        (
            legal,
            &[
                "reviewer_identity",
                "review_date",
                "redistribution_terms",
                "conversion_terms",
                "legal_approval_id",
                "weight_license",
                "code_license",
            ][..],
        ),
        (
            approval,
            &[
                "asset_id",
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
            ][..],
        ),
    ] {
        for field in fields {
            nonempty(record, field)?;
        }
    }
    date(legal, "review_date")?;
    date(approval, "approval_date")?;
    lowercase_sha(candidate, "sha256")?;
    lowercase_sha(approval, "candidate_record_sha256")?;
    lowercase_sha(approval, "legal_review_record_sha256")?;
    let candidate_sha = string(candidate, "sha256")?;
    if string(candidate, "source")? != format!("urn:sha256:{candidate_sha}") {
        return Err("candidate source must be the exact asset SHA-256 URN".to_owned());
    }
    equal(candidate, "asset_id", approval, "asset_id")?;
    equal(candidate, "sha256", approval, "candidate_sha256")?;
    equal(legal, "legal_approval_id", approval, "legal_approval_id")?;
    equal(candidate, "conversion_terms", legal, "conversion_terms")?;
    equal(candidate, "weight_license", legal, "weight_license")?;
    equal(candidate, "code_license", legal, "code_license")?;
    equal(
        legal,
        "redistribution_terms",
        approval,
        "redistribution_terms",
    )?;
    if ![
        string(candidate, "conversion_terms")?,
        string(legal, "conversion_terms")?,
    ]
    .contains(&string(approval, "conversion_terms")?)
    {
        return Err("conversion terms do not match provenance and legal review".to_owned());
    }
    Ok(())
}

fn object<'a>(value: &'a Value, name: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{name} must contain a JSON object"))
}

fn exact_fields(record: &Map<String, Value>, expected: &[&str], name: &str) -> Result<(), String> {
    let actual = record.keys().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{name} fields mismatch"))
    }
}

fn approved(record: &Map<String, Value>, name: &str) -> Result<(), String> {
    if record.get("status").and_then(Value::as_str) == Some("APPROVED") {
        Ok(())
    } else {
        Err(format!("{name} status must be APPROVED"))
    }
}

fn string<'a>(record: &'a Map<String, Value>, field: &str) -> Result<&'a str, String> {
    record
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{field} must be a non-empty string"))
}

fn nonempty(record: &Map<String, Value>, field: &str) -> Result<(), String> {
    if string(record, field)?.trim().is_empty() {
        Err(format!("{field} must be a non-empty string"))
    } else {
        Ok(())
    }
}

fn lowercase_sha(record: &Map<String, Value>, field: &str) -> Result<(), String> {
    let value = string(record, field)?;
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(format!(
            "{field} must be 64 lowercase hexadecimal characters"
        ))
    }
}

fn date(record: &Map<String, Value>, field: &str) -> Result<(), String> {
    let value = string(record, field)?;
    let valid = value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(format!("{field} must use YYYY-MM-DD"))
    }
}

fn equal(
    left: &Map<String, Value>,
    left_field: &str,
    right: &Map<String, Value>,
    right_field: &str,
) -> Result<(), String> {
    if string(left, left_field)? == string(right, right_field)? {
        Ok(())
    } else {
        Err(format!("{right_field} does not match {left_field}"))
    }
}
