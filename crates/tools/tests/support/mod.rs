use std::error::Error;

use realtime_noise_tools::host_evidence::{
    BoundHostEvidence, CpuTopologyEntry, HostEvidenceFiles, LocalHostFacts, RunBinding,
    bind_host_evidence,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const RUN_ID: &str = "host-evidence-test-run";
pub const RUN_STARTED_AT: u64 = 1_000;

pub fn reference_host() -> LocalHostFacts {
    LocalHostFacts {
        os: "linux".to_owned(),
        architecture: "x86_64".to_owned(),
        cpu_model: "Intel(R) Core(TM) i5-10210U CPU".to_owned(),
        physical_cores: Some(4),
        avx2: true,
        ram_kib: Some(8_388_608),
        virtualized: Some(false),
        ac_power: Some(true),
        affinity_cpus: vec![0, 1, 2, 3],
        governor: Some("performance".to_owned()),
        topology: (0..4)
            .map(|logical_cpu| CpuTopologyEntry {
                logical_cpu,
                physical_package_id: 0,
                core_id: logical_cpu,
            })
            .collect(),
    }
}

pub fn provenance_record() -> Result<Vec<u8>, serde_json::Error> {
    let observation_sha256 = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&frequency_observation())?)
    );
    serde_json::to_vec(&json!({
        "schema_version": 1,
        "run_id": RUN_ID,
        "observation_started_at_unix_seconds": 600,
        "observation_finished_at_unix_seconds": 900,
        "collector": "qualification-host-observer 2.0",
        "command": ["qualification-host-observer", "observe", "--seconds", "300"],
        "observation_sha256": observation_sha256
    }))
}

pub fn complete_document(provenance: &[u8]) -> Value {
    let source_sha256 = format!("{:x}", Sha256::digest(provenance));
    json!({
        "schema_version": 2,
        "run_id": RUN_ID,
        "observation_started_at_unix_seconds": 600,
        "observation_finished_at_unix_seconds": 900,
        "host": {
            "os": "linux",
            "architecture": "x86_64",
            "cpu_model": "Intel(R) Core(TM) i5-10210U CPU",
            "physical_cores": 4,
            "avx2": true,
            "ram_kib": 8_388_608,
            "virtualized": false
        },
        "operating_conditions": {
            "ac_power": true,
            "affinity_cpus": [0, 1, 2, 3],
            "governor": "performance"
        },
        "sustained_frequency_observation": frequency_observation(),
        "provenance": {
            "collector": "qualification-host-observer 2.0",
            "command": ["qualification-host-observer", "observe", "--seconds", "300"],
            "source_sha256": source_sha256
        }
    })
}

fn frequency_observation() -> Value {
    json!({"snapshots": frequency_snapshots()})
}

pub fn frequency_snapshots() -> Vec<Value> {
    (0_u64..=300)
        .map(|second| {
            json!({
                "elapsed_milliseconds": second * 1_000,
                "cores": (0..4).map(|logical_cpu| json!({
                    "logical_cpu": logical_cpu,
                    "physical_package_id": 0,
                    "core_id": logical_cpu,
                    "frequency_mhz": 2_100.0 + f64::from(logical_cpu)
                })).collect::<Vec<_>>()
            })
        })
        .collect()
}

pub fn bind(
    document: &Value,
    provenance: &[u8],
    local: &LocalHostFacts,
) -> Result<BoundHostEvidence, Box<dyn Error>> {
    let document = serde_json::to_vec(document)?;
    bind_bytes(&document, provenance, local)
}

pub fn bind_bytes(
    document: &[u8],
    provenance: &[u8],
    local: &LocalHostFacts,
) -> Result<BoundHostEvidence, Box<dyn Error>> {
    let files = HostEvidenceFiles {
        document,
        provenance,
    };
    let binding = RunBinding {
        run_id: RUN_ID,
        started_at_unix_seconds: RUN_STARTED_AT,
    };
    Ok(bind_host_evidence(files, local, &binding)?)
}
