mod support;

use serde_json::Value;
use support::{bind, bind_bytes, complete_document, provenance_record, reference_host};

fn snapshots(document: &mut Value) -> Result<&mut Vec<Value>, std::io::Error> {
    document["sustained_frequency_observation"]["snapshots"]
        .as_array_mut()
        .ok_or_else(|| std::io::Error::other("test snapshots are not an array"))
}

#[test]
fn truncated_frequency_observation_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    snapshots(&mut document)?.pop();

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn sparse_frequency_observation_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    snapshots(&mut document)?.remove(1);

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn duplicated_frequency_timestamp_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    snapshots(&mut document)?[2]["elapsed_milliseconds"] = Value::from(1_000);

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn out_of_order_frequency_timestamp_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    snapshots(&mut document)?[2]["elapsed_milliseconds"] = Value::from(500);

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn non_finite_frequency_sample_is_rejected_at_json_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let document =
        serde_json::to_string(&complete_document(&provenance))?.replacen("2100.0", "1e400", 1);

    assert!(bind_bytes(document.as_bytes(), &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn below_threshold_frequency_sample_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    snapshots(&mut document)?[150]["cores"][2]["frequency_mhz"] = Value::from(1_999.9);

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn wrong_logical_cpu_sample_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    snapshots(&mut document)?[150]["cores"][2]["logical_cpu"] = Value::from(7);

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn wrong_physical_core_topology_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    snapshots(&mut document)?[150]["cores"][2]["core_id"] = Value::from(3);

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn missing_required_core_sample_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    assert!(
        snapshots(&mut document)?[150]["cores"]
            .as_array_mut()
            .is_some_and(|cores| cores.pop().is_some())
    );

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn excessive_frequency_snapshots_are_rejected_during_deserialization()
-> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    let repeated = snapshots(&mut document)?[0].clone();
    snapshots(&mut document)?.resize(302, repeated);

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn affinity_subset_cannot_qualify_partial_topology() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    document["operating_conditions"]["affinity_cpus"] = serde_json::json!([0, 1]);
    for snapshot in snapshots(&mut document)? {
        snapshot["cores"]
            .as_array_mut()
            .ok_or_else(|| std::io::Error::other("test cores are not an array"))?
            .truncate(2);
    }
    let mut local = reference_host();
    local.affinity_cpus = vec![0, 1];

    assert!(bind(&document, &provenance, &local).is_err());
    Ok(())
}

#[test]
fn provenance_must_bind_the_frequency_observation() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    snapshots(&mut document)?[150]["cores"][2]["frequency_mhz"] = Value::from(2_200.0);

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn oversized_evidence_document_is_rejected_before_parsing() -> Result<(), Box<dyn std::error::Error>>
{
    let provenance = provenance_record()?;
    let mut document = serde_json::to_vec(&complete_document(&provenance))?;
    document.resize(16 * 1024 * 1024 + 1, b' ');

    assert!(bind_bytes(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn wrong_physical_package_topology_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let provenance = provenance_record()?;
    let mut document = complete_document(&provenance);
    snapshots(&mut document)?[150]["cores"][2]["physical_package_id"] = Value::from(1);

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}

#[test]
fn oversized_provenance_record_is_rejected_before_parsing() -> Result<(), Box<dyn std::error::Error>>
{
    let mut provenance = provenance_record()?;
    provenance.resize(64 * 1024 + 1, b' ');
    let document = complete_document(&provenance);

    assert!(bind(&document, &provenance, &reference_host()).is_err());
    Ok(())
}
