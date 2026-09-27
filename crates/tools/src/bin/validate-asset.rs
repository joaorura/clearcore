use std::{env, process::ExitCode};

use realtime_noise_model::ApprovedAssetManifest;
use serde_json::json;

fn main() -> ExitCode {
    let root = match env::current_dir() {
        Ok(root) => root,
        Err(error) => return fail(&error.to_string()),
    };
    match ApprovedAssetManifest::verify(&root) {
        Ok(manifest) => {
            println!(
                "{}",
                json!({
                    "status": "M0_APPROVED",
                    "asset_id": manifest.asset_id(),
                    "asset_sha256": manifest.asset_sha256(),
                    "candidate_record_sha256": manifest.candidate_record_sha256(),
                    "legal_review_record_sha256": manifest.legal_review_record_sha256(),
                    "key_id": manifest.key_id(),
                })
            );
            ExitCode::SUCCESS
        }
        Err(error) => fail(&error.to_string()),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!(
        "{}",
        json!({"status": "BLOCKED_NO_APPROVED_ASSET", "reason": message})
    );
    ExitCode::from(2)
}
