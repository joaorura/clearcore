use realtime_noise_tools::host_evidence::{
    HostEvidenceFiles, LocalHostFacts, RunBinding, bind_host_evidence,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const RUN_ID: &str = "host-evidence-test-run";
const RUN_STARTED_AT: u64 = 1_000;

fn reference_host() -> LocalHostFacts {
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
    }
}

fn complete_document(source_sha256: &str) -> Value {
    json!({
        "schema_version": 1,
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
            "minimum_sustained_frequency_mhz": 2_000.0,
            "ac_power": true,
            "affinity_cpus": [0, 1, 2, 3],
            "governor": "performance"
        },
        "provenance": {
            "collector": "qualification-host-observer 1.0",
            "command": ["qualification-host-observer", "observe", "--seconds", "300"],
            "source_description": "300-second host observation log",
            "source_sha256": source_sha256
        }
    })
}

fn bind(document: &Value, provenance: &[u8], local: &LocalHostFacts) -> bool {
    let Ok(document) = serde_json::to_vec(document) else {
        return false;
    };
    let files = HostEvidenceFiles {
        document: &document,
        provenance,
    };
    let binding = RunBinding {
        run_id: RUN_ID,
        started_at_unix_seconds: RUN_STARTED_AT,
    };
    bind_host_evidence(files, local, &binding).is_ok()
}

#[test]
fn complete_fresh_evidence_qualifies_matching_reference_host() {
    let provenance = b"frequency, AC, affinity, and governor observations";
    let digest = format!("{:x}", Sha256::digest(provenance));
    let document = complete_document(&digest);

    assert!(bind(&document, provenance, &reference_host()));
}

#[test]
fn incomplete_evidence_is_rejected() {
    let provenance = b"complete provenance bytes";
    let digest = format!("{:x}", Sha256::digest(provenance));
    let mut document = complete_document(&digest);
    let removed = document["operating_conditions"]
        .as_object_mut()
        .and_then(|conditions| conditions.remove("governor"));
    assert!(removed.is_some());

    assert!(!bind(&document, provenance, &reference_host()));
}

#[test]
fn unknown_evidence_field_is_rejected() {
    let provenance = b"complete provenance bytes";
    let digest = format!("{:x}", Sha256::digest(provenance));
    let mut document = complete_document(&digest);
    document["unexpected"] = Value::Bool(true);

    assert!(!bind(&document, provenance, &reference_host()));
}

#[test]
fn optimistic_evidence_is_rejected_on_mismatched_host() {
    let provenance = b"complete provenance bytes";
    let digest = format!("{:x}", Sha256::digest(provenance));
    let document = complete_document(&digest);
    let mut unsupported = reference_host();
    unsupported.cpu_model = "Intel(R) Core(TM) Ultra 7 265H".to_owned();
    unsupported.physical_cores = Some(16);

    assert!(!bind(&document, provenance, &unsupported));
}

#[test]
fn unknown_virtualization_state_is_rejected() {
    let provenance = b"complete provenance bytes";
    let digest = format!("{:x}", Sha256::digest(provenance));
    let document = complete_document(&digest);
    let mut ambiguous = reference_host();
    ambiguous.virtualized = None;

    assert!(!bind(&document, provenance, &ambiguous));
}

#[test]
fn stale_evidence_is_rejected() {
    let provenance = b"complete provenance bytes";
    let digest = format!("{:x}", Sha256::digest(provenance));
    let mut document = complete_document(&digest);
    document["observation_finished_at_unix_seconds"] = Value::from(699);

    assert!(!bind(&document, provenance, &reference_host()));
}

#[test]
fn invalid_frequency_is_rejected() {
    let provenance = b"complete provenance bytes";
    let digest = format!("{:x}", Sha256::digest(provenance));
    let mut document = complete_document(&digest);
    document["operating_conditions"]["minimum_sustained_frequency_mhz"] = Value::from(-1.0);

    assert!(!bind(&document, provenance, &reference_host()));
}

#[test]
fn non_finite_frequency_is_rejected_at_json_boundary() {
    let provenance = b"complete provenance bytes";
    let digest = format!("{:x}", Sha256::digest(provenance));
    let document = complete_document(&digest)
        .to_string()
        .replace("2000.0", "1e400");
    let files = HostEvidenceFiles {
        document: document.as_bytes(),
        provenance,
    };
    let binding = RunBinding {
        run_id: RUN_ID,
        started_at_unix_seconds: RUN_STARTED_AT,
    };

    assert!(bind_host_evidence(files, &reference_host(), &binding).is_err());
}

#[test]
fn provenance_digest_mismatch_is_rejected() {
    let provenance = b"actual provenance bytes";
    let document = complete_document(&"0".repeat(64));

    assert!(!bind(&document, provenance, &reference_host()));
}
