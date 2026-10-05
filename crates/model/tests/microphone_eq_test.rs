//! Unit and integration tests for microphone spectral calibration and Neural EQ.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::suboptimal_flops,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::float_cmp
)]

use realtime_noise_model::{
    BandGains, MAX_EQ_GAIN_DB, MIN_EQ_GAIN_DB, NUM_ERB_BANDS, VoiceProfile,
    microphone_eq::{
        CALIBRATION_SAMPLE_RATE_HZ, ERB_BAND_WIDTHS, MicrophoneEqConfig, MicrophoneEqError,
        SpeechTargetCurve, embed_eq_in_profile, erb_band_center_frequencies_hz,
        estimate_microphone_eq,
    },
    spectral_eq,
};

/// Synthesizes a test speech-like calibration signal where each of the 32 ERB bands
/// is excited at its center frequency, scaled by the target curve and modified by `mic_filter`.
fn generate_calibration_audio(
    duration_secs: f32,
    sample_rate: u32,
    target: &SpeechTargetCurve,
    mic_filter: impl Fn(f32) -> f32,
) -> Vec<f32> {
    let total_samples = (duration_secs * sample_rate as f32) as usize;
    let freqs = erb_band_center_frequencies_hz();
    let levels = target.levels();

    let pi = std::f64::consts::PI;

    // Fixed deterministic pseudo-random phases
    let mut state: u32 = 0x853c_49e6;
    let mut phases = [0.0_f64; NUM_ERB_BANDS];
    for p in &mut phases {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *p = (f64::from(state >> 8) / 8_388_608.0) * 2.0 * pi;
    }

    // Precompute amplitudes for each band
    let mut amps = [0.0_f64; NUM_ERB_BANDS];
    for b in 0..NUM_ERB_BANDS {
        let width = ERB_BAND_WIDTHS[b] as f64;
        let p_bin = 10.0_f64.powf(f64::from(levels[b]) / 10.0);
        let p_tone = width * p_bin;
        let shape = f64::from(mic_filter(freqs[b]));
        amps[b] = (2.0 * p_tone).sqrt() * shape;
    }

    let mut samples = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let t = (i as f64) / (sample_rate as f64);
        // Syllabic modulation envelope (3.5 Hz) to generate realistic speech frame dynamics
        let env = 0.5 * (1.0 + (2.0 * pi * 3.5 * t).sin());

        let mut val = 0.0_f64;
        for b in 0..NUM_ERB_BANDS {
            val += amps[b] * (2.0 * pi * (freqs[b] as f64) * t + phases[b]).sin();
        }

        // Global scaling to put the audio into comfortable -20 dBFS range
        let s = (0.005 * env * val).clamp(-0.95, 0.95) as f32;
        samples.push(s);
    }

    samples
}

#[test]
fn test_flat_signal_produces_near_zero_neutral_gains() {
    let target = SpeechTargetCurve::broadcast();
    // Neutral mic: mic_filter = 1.0 (flat response across all frequencies)
    let audio = generate_calibration_audio(2.0, CALIBRATION_SAMPLE_RATE_HZ, &target, |_| 1.0);
    let config = MicrophoneEqConfig::default();

    let gains = estimate_microphone_eq(&audio, CALIBRATION_SAMPLE_RATE_HZ, &target, &config)
        .expect("estimation should succeed");

    assert_eq!(gains.gains_db.len(), NUM_ERB_BANDS);
    gains.validate().expect("gains must be valid");

    // All core speech bands (bands 2..22) must be within 0.8 dB of 0.0 dB (neutral)
    for b in 2..22 {
        assert!(
            gains.gains_db[b].abs() < 0.8,
            "band {b} gain {} dB is not close to 0 dB",
            gains.gains_db[b]
        );
    }
}

#[test]
fn test_flat_stimulus_against_flat_target_produces_neutral() {
    let target = SpeechTargetCurve::flat();
    let audio = generate_calibration_audio(2.0, CALIBRATION_SAMPLE_RATE_HZ, &target, |_| 1.0);
    let config = MicrophoneEqConfig::default();

    let gains = estimate_microphone_eq(&audio, CALIBRATION_SAMPLE_RATE_HZ, &target, &config)
        .expect("estimation should succeed");

    gains.validate().expect("must validate");
    for (b, &g) in gains.gains_db.iter().enumerate() {
        assert!(
            g.abs() < 1.0,
            "band {b} gain {g} is not near 0 dB for flat stimulus"
        );
    }
}

#[test]
fn test_treble_deficiency_receives_smooth_boost_within_safe_bounds() {
    let target = SpeechTargetCurve::broadcast();
    // Simulate muffled microphone: smooth low-pass roll-off with fc = 2500 Hz
    let audio = generate_calibration_audio(2.0, CALIBRATION_SAMPLE_RATE_HZ, &target, |f| {
        let fc = 2500.0_f32;
        1.0 / (1.0 + (f / fc).powi(2)).sqrt()
    });

    let config = MicrophoneEqConfig::default();
    let gains = estimate_microphone_eq(&audio, CALIBRATION_SAMPLE_RATE_HZ, &target, &config)
        .expect("estimation should succeed");

    gains.validate().expect("must validate");

    // High bands (> 2.5 kHz, bands 17..24) must receive a positive gain boost
    for b in 18..=24 {
        assert!(
            gains.gains_db[b] > 0.5,
            "band {b} should have positive boost for treble deficiency, got {}",
            gains.gains_db[b]
        );
    }

    // Must be strictly within [-6.0, +12.0] dB
    for (b, &g) in gains.gains_db.iter().enumerate() {
        assert!(
            (MIN_EQ_GAIN_DB..=MAX_EQ_GAIN_DB).contains(&g),
            "band {b} gain {g} violates safe range [{MIN_EQ_GAIN_DB}, {MAX_EQ_GAIN_DB}]"
        );
    }

    // Must be smooth between adjacent bands (no sharp steps > 3.0 dB)
    for w in gains.gains_db.windows(2) {
        let diff = (w[1] - w[0]).abs();
        assert!(
            diff < 3.0,
            "excessive step {diff:.2} dB between adjacent bands: {} and {}",
            w[0],
            w[1]
        );
    }
}

#[test]
fn test_bass_deficiency_receives_smooth_boost_within_safe_bounds() {
    let target = SpeechTargetCurve::broadcast();
    // Simulate tinny laptop mic: smooth high-pass roll-off with fc = 350 Hz
    let audio = generate_calibration_audio(2.0, CALIBRATION_SAMPLE_RATE_HZ, &target, |f| {
        let fc = 350.0_f32;
        (f / fc) / (1.0 + (f / fc).powi(2)).sqrt()
    });

    let config = MicrophoneEqConfig::default();
    let gains = estimate_microphone_eq(&audio, CALIBRATION_SAMPLE_RATE_HZ, &target, &config)
        .expect("estimation should succeed");

    gains.validate().expect("must validate");

    // Low-mid bands (bands 1..3: 100 Hz to 400 Hz) should receive a positive boost
    assert!(
        gains.gains_db[1] > 0.5 || gains.gains_db[2] > 0.5,
        "low bands should be boosted to restore warmth, got b1={}, b2={}",
        gains.gains_db[1],
        gains.gains_db[2]
    );

    // All bands strictly bounded
    for (b, &g) in gains.gains_db.iter().enumerate() {
        assert!(
            (MIN_EQ_GAIN_DB..=MAX_EQ_GAIN_DB).contains(&g),
            "band {b} gain {g} outside safe range"
        );
    }

    // Smoothness check
    for w in gains.gains_db.windows(2) {
        let diff = (w[1] - w[0]).abs();
        assert!(
            diff < 3.0,
            "excessive step {diff:.2} dB between adjacent bands"
        );
    }
}

#[test]
fn test_boomy_signal_receives_gentle_cut() {
    let target = SpeechTargetCurve::broadcast();
    // Proximity effect: boost around 200..400 Hz (fc = 300 Hz bell)
    let audio = generate_calibration_audio(2.0, CALIBRATION_SAMPLE_RATE_HZ, &target, |f| {
        let bell = (-((f - 300.0) / 150.0).powi(2)).exp();
        1.0 + 1.5 * bell // +8 dB peak at 300 Hz
    });

    let config = MicrophoneEqConfig::default();
    let gains = estimate_microphone_eq(&audio, CALIBRATION_SAMPLE_RATE_HZ, &target, &config)
        .expect("estimation should succeed");

    gains.validate().expect("must validate");

    // Low-mid bands around 300 Hz (bands 2, 3) should have negative gain (cut)
    assert!(
        gains.gains_db[2] < 0.0 || gains.gains_db[3] < 0.0,
        "boomy bands should be attenuated, got band2={} band3={}",
        gains.gains_db[2],
        gains.gains_db[3]
    );

    // Bounded by MIN_EQ_GAIN_DB (-6.0 dB)
    for (b, &g) in gains.gains_db.iter().enumerate() {
        assert!(
            g >= MIN_EQ_GAIN_DB,
            "band {b} cut {g} exceeded minimum {MIN_EQ_GAIN_DB} dB"
        );
    }
}

#[test]
fn test_parity_with_bin_factors_and_gain_validation() {
    let target = SpeechTargetCurve::broadcast();
    let audio = generate_calibration_audio(2.0, CALIBRATION_SAMPLE_RATE_HZ, &target, |f| {
        let fc = 3000.0_f32;
        1.0 / (1.0 + (f / fc).powi(2)).sqrt()
    });

    let config = MicrophoneEqConfig::default();
    let gains = estimate_microphone_eq(&audio, CALIBRATION_SAMPLE_RATE_HZ, &target, &config)
        .expect("estimate eq");

    // 1. Must pass BandGains::validate
    gains.validate().expect("BandGains must be strictly valid");

    // 2. Must produce valid 481 bin factors via spectral_eq::bin_factors
    let factors = spectral_eq::bin_factors(&gains, &ERB_BAND_WIDTHS)
        .expect("bin_factors must succeed with ERB_BAND_WIDTHS");

    assert_eq!(factors.len(), ERB_BAND_WIDTHS.iter().sum::<usize>());
    assert_eq!(factors.len(), 481);

    // Factors must be strictly positive and bounded by [-6, +12] dB linear equivalents
    let min_factor = 10.0_f32.powf(MIN_EQ_GAIN_DB / 20.0); // ~0.501
    let max_factor = 10.0_f32.powf(MAX_EQ_GAIN_DB / 20.0); // ~3.981

    for (k, &f) in factors.iter().enumerate() {
        assert!(f.is_finite(), "bin {k} factor is non-finite");
        assert!(
            f >= min_factor - 1e-4 && f <= max_factor + 1e-4,
            "bin {k} factor {f} outside [{min_factor}, {max_factor}]"
        );
    }

    // 3. Parity at band center bins
    let mut bin_start = 0_usize;
    for (b, &width) in ERB_BAND_WIDTHS.iter().enumerate() {
        let center_bin = bin_start + width / 2;
        let factor_db = 20.0 * factors[center_bin].log10();
        let target_db = gains.gains_db[b];
        assert!(
            (factor_db - target_db).abs() < 1.0,
            "band {b} center bin {center_bin}: factor {factor_db:.2} dB vs band gain {target_db:.2} dB"
        );
        bin_start += width;
    }
}

#[test]
fn test_resampling_16k_enrollment_audio_and_high_band_decay() {
    let target = SpeechTargetCurve::broadcast();
    let rate_16k = 16_000_u32;
    // 2 seconds of 16 kHz audio (32,000 samples)
    let audio_16k = generate_calibration_audio(2.0, rate_16k, &target, |_| 1.0);

    let config = MicrophoneEqConfig::default();
    let gains = estimate_microphone_eq(&audio_16k, rate_16k, &target, &config)
        .expect("16 kHz audio estimation should succeed");

    gains.validate().expect("gains must be valid");

    // Unobserved bands above Nyquist (8 kHz -> bands 25..31) must have decayed to 0.0 dB
    for b in 25..32 {
        assert_eq!(
            gains.gains_db[b], 0.0,
            "unobserved band {b} above 8 kHz must decay to 0.0 dB, got {}",
            gains.gains_db[b]
        );
    }
}

#[test]
fn test_embed_eq_in_voice_profile_integrity() {
    let mut profile =
        VoiceProfile::identity("spk-eq-test", "Alice", "2026-10-05T00:00:00Z").unwrap();
    let initial_hash = profile.integrity_hash.clone();
    assert!(profile.is_neutral());

    let mut raw_gains = [0.0_f32; NUM_ERB_BANDS];
    raw_gains[5] = 2.5;
    raw_gains[18] = 4.0;
    raw_gains[23] = -1.2;
    let gains = BandGains::from_array(raw_gains).unwrap();

    embed_eq_in_profile(&mut profile, gains.clone()).expect("embed_eq_in_profile");

    assert_eq!(profile.eq.as_ref(), Some(&gains));
    assert_ne!(profile.integrity_hash, initial_hash);
    profile
        .verify_integrity()
        .expect("profile integrity check must pass");

    // Verify round-trip JSON serialization
    let json = profile.to_json().expect("to_json");
    let loaded = VoiceProfile::from_json(&json).expect("from_json");
    assert_eq!(profile, loaded);
    assert_eq!(loaded.eq.as_ref(), Some(&gains));
}

#[test]
fn test_error_handling_cases() {
    let target = SpeechTargetCurve::broadcast();
    let config = MicrophoneEqConfig::default();

    // 1. Empty audio
    assert!(matches!(
        estimate_microphone_eq(&[], 48_000, &target, &config),
        Err(MicrophoneEqError::EmptyAudio)
    ));

    // 2. Invalid sample rate
    assert!(matches!(
        estimate_microphone_eq(&[0.1; 2000], 0, &target, &config),
        Err(MicrophoneEqError::InvalidSampleRate(0))
    ));

    // 3. Non-finite sample
    let mut bad_samples = vec![0.1_f32; 2000];
    bad_samples[100] = f32::NAN;
    assert!(matches!(
        estimate_microphone_eq(&bad_samples, 48_000, &target, &config),
        Err(MicrophoneEqError::NonFiniteSample { index: 100 })
    ));

    // 4. Too short
    assert!(matches!(
        estimate_microphone_eq(&[0.1; 500], 48_000, &target, &config),
        Err(MicrophoneEqError::TooShort { .. })
    ));

    // 5. Silent audio
    let silence = vec![0.0_f32; 48_000];
    assert!(matches!(
        estimate_microphone_eq(&silence, 48_000, &target, &config),
        Err(MicrophoneEqError::TooQuiet { .. })
    ));
}
