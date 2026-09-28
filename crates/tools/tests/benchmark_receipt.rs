use serde_json::Value;

#[test]
fn blocked_receipt_declares_ready_allocation_instrumentation()
-> Result<(), Box<dyn std::error::Error>> {
    let receipt: Value =
        serde_json::from_str(include_str!("../../../benchmarks/cpu-baseline.json"))?;
    let allocation = &receipt["measurements"]["allocation"];

    // In the offline build, allocation instrumentation is unavailable because stats_alloc
    // is not in the offline cache. A network-enabled dependency bootstrap is required
    // to fetch stats_alloc=0.1.10 and enable the alloc_instrumentation feature.
    assert_eq!(
        allocation["mechanism"],
        "unavailable-under-unsafe-code-forbid"
    );
    assert_eq!(allocation["available"], false);
    assert!(allocation["per_hop_count"].is_null());
    assert!(allocation["per_hop_bytes"].is_null());
    Ok(())
}
