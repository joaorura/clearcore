//! Owned by task M2; see docs/superpowers/plans/2026-10-05-voice-enrollment-pipeline-service.md
//!
//! 48 kHz to 16 kHz decimator: windowed-sinc low-pass FIR followed by
//! keeping every third sample, with the filter group delay compensated.

/// Decimates 48 kHz audio to 16 kHz. Output length is `input.len() / 3`.
pub fn decimate_48k_to_16k(input: &[f32]) -> Vec<f32> {
    let h = lowpass_taps();
    let m = (TAPS - 1) / 2;
    let out_len = input.len() / FACTOR;
    let mut out = Vec::with_capacity(out_len);
    for k in 0..out_len {
        let center = k * FACTOR + m;
        // x index = center - j must lie in [0, len): j in [center - len + 1, center].
        let j_lo = (center + 1).saturating_sub(input.len());
        let j_hi = center.min(TAPS - 1);
        let mut acc = 0.0_f32;
        for (j, &coef) in h.iter().enumerate().take(j_hi + 1).skip(j_lo) {
            if let Some(&x) = input.get(center - j) {
                acc += coef * x;
            }
        }
        out.push(acc);
    }
    out
}

const FACTOR: usize = 3;
const TAPS: usize = 127;
/// Cutoff in cycles per sample at 48 kHz.
const CUTOFF: f64 = 7_000.0 / 48_000.0;

/// Windowed-sinc low-pass (4-term Blackman-Harris), normalised to unit DC gain.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::suboptimal_flops
)]
fn lowpass_taps() -> Vec<f32> {
    use std::f64::consts::PI;
    let m = (TAPS - 1) as f64 / 2.0;
    let denom = (TAPS - 1) as f64;
    let mut h: Vec<f64> = (0..TAPS)
        .map(|n| {
            let t = n as f64 - m;
            let arg = 2.0 * CUTOFF * t;
            let sinc = if arg == 0.0 {
                1.0
            } else {
                (PI * arg).sin() / (PI * arg)
            };
            let p = 2.0 * PI * n as f64 / denom;
            let w =
                0.35875 - 0.48829 * p.cos() + 0.14128 * (2.0 * p).cos() - 0.01168 * (3.0 * p).cos();
            2.0 * CUTOFF * sinc * w
        })
        .collect();
    let sum: f64 = h.iter().sum();
    for v in &mut h {
        *v /= sum;
    }
    h.into_iter().map(|v| v as f32).collect()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    clippy::many_single_char_names
)]
mod tests {
    use super::*;

    fn sine(freq: f32, secs: f32) -> Vec<f32> {
        (0..(48_000.0 * secs) as usize)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / 48_000.0).sin())
            .collect()
    }
    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }
    fn tone_db(freq: f32) -> f32 {
        let y = decimate_48k_to_16k(&sine(freq, 1.0));
        20.0 * (rms(&y[400..y.len() - 400]) / (1.0 / 2f32.sqrt())).log10()
    }

    #[test]
    fn length_is_one_third() {
        assert_eq!(decimate_48k_to_16k(&vec![0.0; 48_000]).len(), 16_000);
    }
    #[test]
    fn passband_tone_keeps_its_level() {
        let db = tone_db(1_000.0);
        eprintln!("1 kHz: {db} dB");
        assert!(db.abs() < 0.5, "{db} dB");
    }
    #[test]
    fn alias_band_tone_is_rejected() {
        // 10 kHz would alias to 6 kHz at 16 kHz if not filtered.
        let db = tone_db(10_000.0);
        eprintln!("10 kHz: {db} dB");
        assert!(db < -60.0, "{db} dB");
    }
    #[test]
    fn dc_gain_is_unity() {
        let y = decimate_48k_to_16k(&vec![0.25; 48_000]);
        assert!((y[8_000] - 0.25).abs() < 1e-3);
    }
    #[test]
    fn chirp_passband_gain_is_within_one_db() {
        // Sweep of 100 Hz..6 kHz (the cutoff is 7 kHz, the -6 dB point of a
        // windowed sinc, so the last kHz below it rolls off by design).
        let secs = 4.0_f32;
        let n = (48_000.0 * secs) as usize;
        let (f0, f1) = (100.0_f32, 6_000.0_f32);
        let k = (f1 - f0) / secs;
        let x: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                (2.0 * std::f32::consts::PI * (f0 * t + 0.5 * k * t * t)).sin()
            })
            .collect();
        let y = decimate_48k_to_16k(&x);
        let win = 1_600; // 0.1 s at 16 kHz
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        let mut s = 800;
        while s + win <= y.len() - 800 {
            let db = 20.0 * (rms(&y[s..s + win]) / (1.0 / 2f32.sqrt())).log10();
            lo = lo.min(db);
            hi = hi.max(db);
            s += win;
        }
        eprintln!("chirp gain: min {lo} dB, max {hi} dB");
        assert!(lo > -1.0 && hi < 1.0, "min {lo} dB, max {hi} dB");
    }
}
