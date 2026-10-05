//! Speech trimming, gain matching and crossfade joining for enrollment audio.
//! Owned by task M1; see docs/superpowers/plans/2026-10-05-voice-enrollment-pipeline-service.md

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

/// Result of [`trim_speech`].
#[derive(Debug, Clone, PartialEq)]
pub struct TrimmedSpeech {
    pub samples: Vec<f32>,
    pub speech_seconds: f32,
    pub active_fraction: f32,
}

const FLOOR_DBFS: f32 = -50.0;
const RANGE_DB: f32 = 30.0;
const MARGIN_FRAMES: usize = 2;
const JOIN_FADE_MS: u32 = 20;

struct FrameActivity {
    frame: usize,
    active: Vec<bool>,
}

fn analyze(samples: &[f32], sample_rate: u32) -> FrameActivity {
    let frame = ((sample_rate / 50) as usize).max(1);
    let dbfs: Vec<f32> = samples
        .chunks_exact(frame)
        .map(|c| {
            let ms = c.iter().map(|v| v * v).sum::<f32>() / frame as f32;
            10.0 * (ms + 1e-12).log10()
        })
        .collect();
    let loudest = dbfs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let thr = FLOOR_DBFS.max(loudest - RANGE_DB);
    let active = dbfs.iter().map(|d| *d >= thr).collect();
    FrameActivity { frame, active }
}

/// 20 ms frames, threshold max(-50 dBFS, loudest - 30 dB) on frame mean-square, each active run
/// kept with a 40 ms margin on both sides, runs joined with a 20 ms linear crossfade.
#[must_use]
pub fn trim_speech(samples: &[f32], sample_rate: u32) -> TrimmedSpeech {
    let a = analyze(samples, sample_rate);
    let n = a.active.len();
    let count = a.active.iter().filter(|v| **v).count();
    if n == 0 || count == 0 {
        return TrimmedSpeech {
            samples: Vec::new(),
            speech_seconds: 0.0,
            active_fraction: 0.0,
        };
    }
    let dilated: Vec<bool> = (0..n)
        .map(|i| {
            let lo = i.saturating_sub(MARGIN_FRAMES);
            let hi = (i + MARGIN_FRAMES).min(n - 1);
            a.active[lo..=hi].iter().any(|v| *v)
        })
        .collect();
    let mut runs: Vec<&[f32]> = Vec::new();
    let mut i = 0;
    while i < n {
        if dilated[i] {
            let start = i;
            while i < n && dilated[i] {
                i += 1;
            }
            runs.push(&samples[start * a.frame..i * a.frame]);
        } else {
            i += 1;
        }
    }
    let out = join_crossfade(&runs, sample_rate, JOIN_FADE_MS);
    let speech_seconds = out.len() as f32 / sample_rate as f32;
    TrimmedSpeech {
        samples: out,
        speech_seconds,
        active_fraction: count as f32 / n as f32,
    }
}

/// RMS in dBFS over active frames only (same VAD rule); -120.0 when nothing is active.
#[must_use]
pub fn active_rms_dbfs(samples: &[f32], sample_rate: u32) -> f32 {
    let a = analyze(samples, sample_rate);
    let mut sum = 0.0_f64;
    let mut count = 0_usize;
    for (idx, chunk) in samples.chunks_exact(a.frame).enumerate() {
        if a.active.get(idx).copied().unwrap_or(false) {
            sum += chunk
                .iter()
                .map(|v| f64::from(*v) * f64::from(*v))
                .sum::<f64>();
            count += a.frame;
        }
    }
    if count == 0 {
        return -120.0;
    }
    (10.0 * (sum / count as f64 + 1e-12).log10()) as f32
}

/// Multiply by `10^(gain_db/20)` with `gain_db` clamped to `±max_abs_db`.
#[must_use]
pub fn apply_gain_limited(samples: &[f32], gain_db: f32, max_abs_db: f32) -> Vec<f32> {
    let limit = max_abs_db.abs();
    let g = 10f32.powf(gain_db.clamp(-limit, limit) / 20.0);
    samples.iter().map(|v| v * g).collect()
}

/// Join parts with a linear crossfade of `crossfade_ms`; parts shorter than the fade are copied whole.
#[must_use]
pub fn join_crossfade(parts: &[&[f32]], sample_rate: u32, crossfade_ms: u32) -> Vec<f32> {
    let fade = (u64::from(sample_rate) * u64::from(crossfade_ms) / 1000) as usize;
    let mut out: Vec<f32> = Vec::new();
    for part in parts.iter().filter(|p| !p.is_empty()) {
        if fade == 0 || out.len() < fade || part.len() < fade {
            out.extend_from_slice(part);
            continue;
        }
        let base = out.len() - fade;
        for (k, p) in part[..fade].iter().enumerate() {
            let t = (k as f32 + 1.0) / (fade as f32 + 1.0);
            out[base + k] = out[base + k].mul_add(1.0 - t, p * t);
        }
        out.extend_from_slice(&part[fade..]);
    }
    out
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::suboptimal_flops
)]
mod tests {
    use super::*;

    fn tone(len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| amp * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 48_000.0).sin())
            .collect()
    }

    #[test]
    fn silence_around_a_burst_is_removed_but_margins_stay() {
        let mut x = vec![0.0; 48_000];
        x.extend(tone(24_000, 0.3));
        x.extend(vec![0.0; 48_000]);
        let t = trim_speech(&x, 48_000);
        let secs = t.samples.len() as f32 / 48_000.0;
        assert!(
            (0.5 + 0.08 - 0.03..=0.5 + 0.08 + 0.03).contains(&secs),
            "got {secs}"
        );
        assert!((t.speech_seconds - secs).abs() < 1e-6);
    }

    #[test]
    fn pure_silence_gives_zero_speech() {
        let t = trim_speech(&vec![0.0; 48_000], 48_000);
        assert!(t.samples.is_empty() && t.speech_seconds == 0.0 && t.active_fraction == 0.0);
    }

    #[test]
    fn two_bursts_are_joined_without_a_jump() {
        let mut x = tone(9_600, 0.3);
        x.extend(vec![0.0; 48_000]);
        x.extend(tone(9_600, 0.3));
        let t = trim_speech(&x, 48_000);
        let max_step = t
            .samples
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        assert!(max_step < 0.1, "step {max_step}");
    }

    #[test]
    fn gain_is_limited() {
        let y = apply_gain_limited(&[0.1], 30.0, 12.0);
        assert!((y[0] - 0.1 * 10f32.powf(12.0 / 20.0)).abs() < 1e-5);
    }

    #[test]
    fn crossfade_join_has_expected_length_and_no_clicks() {
        let a = vec![0.5; 4_800];
        let b = vec![0.5; 4_800];
        let j = join_crossfade(&[&a, &b], 48_000, 20);
        assert_eq!(j.len(), 4_800 + 4_800 - 960);
        assert!(j.iter().all(|v| (*v - 0.5).abs() < 1e-5));
    }

    #[test]
    fn active_rms_ignores_silence_and_reports_floor_when_empty() {
        let mut x = vec![0.0; 48_000];
        x.extend(tone(48_000, 0.3));
        let db = active_rms_dbfs(&x, 48_000);
        assert!(
            (db - 20.0 * (0.3_f32 / 2f32.sqrt()).log10()).abs() < 0.5,
            "{db}"
        );
        assert_eq!(active_rms_dbfs(&vec![0.0; 48_000], 48_000), -120.0);
    }
}
