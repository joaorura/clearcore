#![forbid(unsafe_code)]

//! O `studio-dsp` não depende de `contracts`; esta paridade impede que as duas constantes divirjam.

#[test]
fn studio_dsp_hop_matches_the_audio_contract() {
    assert_eq!(
        studio_dsp::HOP_SAMPLES,
        realtime_noise_contracts::HOP_SAMPLES
    );
}

#[test]
fn studio_dsp_sample_rate_matches_the_audio_contract() {
    assert_eq!(
        studio_dsp::SAMPLE_RATE_HZ,
        realtime_noise_contracts::SAMPLE_RATE_HZ
    );
}
