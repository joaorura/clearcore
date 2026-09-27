use std::path::PathBuf;

use realtime_noise_model::{GoldenFixture, InferenceError};

#[test]
fn missing_frozen_golden_is_a_truthful_block() -> Result<(), Box<dyn std::error::Error>> {
    let missing = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/golden/frozen-reference.json");
    let Err(error) = GoldenFixture::read(&missing) else {
        return Err("no frozen golden may be committed before qualification".into());
    };

    assert!(matches!(error, InferenceError::GoldenPending(_)));
    Ok(())
}
