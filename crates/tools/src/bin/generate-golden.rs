use std::{env, path::Path, process::ExitCode};

use realtime_noise_model::ApprovedAssetManifest;
use serde_json::json;

fn main() -> ExitCode {
    let root = match env::current_dir() {
        Ok(root) => root,
        Err(error) => return blocked("BLOCKED_NO_APPROVED_ASSET", &error.to_string()),
    };
    if let Err(error) = ApprovedAssetManifest::verify(&root) {
        return blocked("BLOCKED_NO_APPROVED_ASSET", &error.to_string());
    }
    if !Path::new("fixtures/corpus/corpus-manifest.json").is_file() {
        return blocked(
            "BLOCKED_PENDING_GOLDEN",
            "validated local corpus manifest is absent; no fixture was generated",
        );
    }
    blocked(
        "BLOCKED_PENDING_GOLDEN",
        "isolated generation host and frozen provenance are not available",
    )
}

fn blocked(status: &str, reason: &str) -> ExitCode {
    eprintln!("{}", json!({"status": status, "reason": reason}));
    ExitCode::from(2)
}
