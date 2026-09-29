mod support;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use support::{bind, complete_document, provenance_record, reference_host};

#[test]
fn structured_fresh_evidence_qualifies_matching_reference_host()
-> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let document = complete_document(&provenance);

    let evidence = bind(&document, &provenance, &reference_host())?;

    assert!((evidence.sustained_frequency.minimum_mhz - 2_100.0).abs() < f64::EPSILON);
    assert_eq!(evidence.sustained_frequency.snapshot_count, 301);
    assert_eq!(evidence.sustained_frequency.maximum_gap_milliseconds, 1_000);
    assert_eq!(
        evidence.sustained_frequency.sampled_logical_cpus,
        vec![0, 1, 2, 3]
    );
    Ok(())
}

#[test]
fn opaque_provenance_and_claimed_minimum_do_not_qualify() {
    let provenance = b"frequency, AC, affinity, and governor observations";
    let digest = format!("{:x}", Sha256::digest(provenance));
    let mut document = complete_document(provenance);
    document["operating_conditions"]["minimum_sustained_frequency_mhz"] = Value::from(2_000.0);
    assert!(
        document
            .as_object_mut()
            .is_some_and(|object| object.remove("sustained_frequency_observation").is_some())
    );
    document["provenance"] = json!({
        "collector": "qualification-host-observer 1.0",
        "command": ["qualification-host-observer", "observe", "--seconds", "300"],
        "source_sha256": digest
    });

    assert!(bind(&document, provenance, &reference_host()).is_err());
}

#[test]
fn incomplete_operating_conditions_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    assert!(
        document["operating_conditions"]
            .as_object_mut()
            .is_some_and(|conditions| conditions.remove("governor").is_some())
    );

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn unknown_evidence_field_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    document["unexpected"] = Value::Bool(true);

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn optimistic_evidence_is_rejected_on_mismatched_host() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let document = complete_document(&provenance);
    let mut unsupported = reference_host();
    unsupported.cpu_model = "Intel(R) Core(TM) Ultra 7 265H".to_owned();
    unsupported.physical_cores = Some(16);

    assert!(bind(&document, &provenance, &unsupported).is_err());
    Ok(())
}

#[test]
fn unknown_virtualization_state_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let document = complete_document(&provenance);
    let mut ambiguous = reference_host();
    ambiguous.virtualized = None;

    assert!(bind(&document, &provenance, &ambiguous).is_err());
    Ok(())
}

#[test]
fn stale_evidence_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    document["observation_started_at_unix_seconds"] = Value::from(398);
    document["observation_finished_at_unix_seconds"] = Value::from(698);

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn run_mismatched_evidence_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    document["run_id"] = Value::from("another-run");

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn provenance_digest_mismatch_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    document["provenance"]["source_sha256"] = Value::from("0".repeat(64));

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}
