use serde_json::Value;

#[test]
fn blocked_receipt_declares_ready_allocation_instrumentation()
-> Result<(), Box<dyn std::error::Error>> {
    let receipt: Value =
        serde_json::from_str(include_str!("../../../benchmarks/cpu-baseline.json"))?;
    let allocation = &receipt["measurements"]["allocation"];

    assert_eq!(receipt["status"], "BLOCKED_UNSUPPORTED_CPU_PROFILE");
    assert_eq!(receipt["clean_worktree"], false);
    assert_eq!(receipt["host"]["reference_class_qualified"], false);
    assert_eq!(receipt["host"]["operating_conditions_observed"], false);
    assert_eq!(receipt["host"]["host_evidence_sha256"], Value::Null);
    assert_eq!(receipt["host"]["host_evidence_run_id"], Value::Null);
    assert_eq!(receipt["host"]["sustained_frequency"], Value::Null);
    assert!(receipt["host"]["topology"].is_array());
    assert_eq!(
        receipt["host"]["rejection_reason"],
        "host evidence was not supplied"
    );
    assert_eq!(allocation["mechanism"], "stats_alloc-0.1.10");
    assert_eq!(allocation["available"], true);
    assert_eq!(allocation["measurement_completed"], false);
    assert_ne!(receipt["status"], "BLOCKED_ALLOCATION_MEASUREMENT");
    assert!(allocation.as_object().is_some_and(|fields| {
        fields.contains_key("verification_count") && fields.contains_key("verification_bytes")
    }));
    assert!(allocation["verification_count"].is_null());
    assert!(allocation["verification_bytes"].is_null());
    assert!(allocation["initialization_count"].is_null());
    assert!(allocation["initialization_bytes"].is_null());
    assert!(allocation["warm_up_count"].is_null());
    assert!(allocation["warm_up_bytes"].is_null());
    assert!(allocation["total_count"].is_null());
    assert!(allocation["total_bytes"].is_null());
    assert!(allocation["per_hop_count"].is_null());
    assert!(allocation["per_hop_bytes"].is_null());
    Ok(())
}
