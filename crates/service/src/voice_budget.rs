//! Owned by task S1; see docs/superpowers/plans/2026-10-05-voice-enrollment-pipeline-service.md

use crate::voice_samples::VoiceSample;

pub const MAX_SPEECH_SECONDS: f32 = 90.0;
pub const MIN_TAKE_MARGIN_SECONDS: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_field_names)] // field names are the IPC/plan contract
pub struct Budget {
    pub used_seconds: f32,
    pub max_seconds: f32,
    pub remaining_seconds: f32,
}

/// Device group of the most recent counting sample (active and with audio), by numeric
/// `timestamp`, ties broken by the later list position.
///
/// `timestamp` contract: a numeric string (epoch in milliseconds) written by the service;
/// non-numeric values count as 0.
#[must_use]
pub fn selected_device_hash(samples: &[VoiceSample]) -> Option<String> {
    samples
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_active && s.audio_path.is_some())
        .max_by_key(|(i, s)| (s.timestamp.parse::<u64>().unwrap_or(0), *i))
        .map(|(_, s)| s.device_id_hash.clone())
}

#[must_use]
pub fn budget_for(samples: &[VoiceSample], device_hash: &str) -> Budget {
    let target_label = samples
        .iter()
        .filter(|s| s.is_active && s.device_id_hash == device_hash && s.audio_path.is_some())
        .max_by_key(|s| s.timestamp.parse::<u64>().unwrap_or(0))
        .map(|s| s.device_label.trim().to_ascii_lowercase());

    let used: f32 = samples
        .iter()
        .filter(|s| {
            s.is_active
                && s.audio_path.is_some()
                && (s.device_id_hash == device_hash
                    || match &target_label {
                        Some(lbl) if !lbl.is_empty() => {
                            s.device_label.trim().to_ascii_lowercase() == *lbl
                        }
                        _ => false,
                    })
        })
        .map(|s| {
            let v = s.speech_seconds;
            if v.is_finite() && v > 0.0 { v } else { 0.0 }
        })
        .sum();
    Budget {
        used_seconds: used,
        max_seconds: MAX_SPEECH_SECONDS,
        remaining_seconds: (MAX_SPEECH_SECONDS - used).max(0.0),
    }
}

#[must_use]
pub fn fits_manual(b: &Budget, speech_seconds: f32) -> bool {
    speech_seconds.is_finite()
        && speech_seconds >= 0.0
        && b.used_seconds + speech_seconds <= b.max_seconds + 1e-4
}

#[must_use]
pub fn fits_take(b: &Budget, speech_seconds: f32) -> bool {
    speech_seconds.is_finite()
        && speech_seconds >= 0.0
        && b.remaining_seconds >= MIN_TAKE_MARGIN_SECONDS
        && speech_seconds <= b.remaining_seconds + 1e-4
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::voice_samples::VoiceSample;

    fn s(id: &str, ts: &str, dev: &str, secs: f32, wav: bool) -> VoiceSample {
        let mut v = VoiceSample::new(id, ts, id, None, vec![0.0; 192]).expect("valid sample");
        v.device_id_hash = dev.to_string();
        v.speech_seconds = secs;
        v.audio_path = wav.then(|| "x.wav".to_string());
        v
    }

    fn budget_used(used: f32) -> Budget {
        budget_for(&[s("a", "1", "A", used, true)], "A")
    }

    #[test]
    fn sums_only_active_samples_of_the_group_with_audio() {
        let list = [
            s("1", "1", "A", 30.0, true),
            s("2", "2", "A", 20.0, true),
            s("3", "3", "B", 40.0, true),
            s("4", "4", "A", 10.0, false),
        ];
        let b = budget_for(&list, "A");
        assert!((b.used_seconds - 50.0).abs() < 1e-4);
        assert!((b.remaining_seconds - 40.0).abs() < 1e-4);
    }

    #[test]
    fn most_recent_sample_selects_the_group() {
        let list = [s("1", "10", "A", 1.0, true), s("2", "20", "B", 1.0, true)];
        assert_eq!(selected_device_hash(&list).as_deref(), Some("B"));
    }

    #[test]
    fn manual_sample_must_fit_exactly_or_less() {
        let b = budget_used(80.0);
        assert!(fits_manual(&b, 10.0));
        assert!(!fits_manual(&b, 10.01));
    }

    #[test]
    fn take_needs_five_seconds_margin_and_must_fit() {
        let b = budget_used(86.0);
        assert!(!fits_take(&b, 1.0));
        let b = budget_used(80.0);
        assert!(fits_take(&b, 6.0));
        assert!(!fits_take(&b, 11.0));
    }

    #[test]
    fn remaining_never_negative() {
        let b = budget_used(95.0);
        assert!(b.remaining_seconds.abs() < 1e-6);
    }

    #[test]
    fn non_numeric_timestamps_count_as_zero() {
        let list = [s("1", "abc", "A", 1.0, true), s("2", "5", "B", 1.0, true)];
        assert_eq!(selected_device_hash(&list).as_deref(), Some("B"));
        let list = [s("1", "abc", "A", 1.0, true), s("2", "", "B", 1.0, true)];
        assert_eq!(selected_device_hash(&list).as_deref(), Some("B"));
    }

    #[test]
    fn inactive_most_recent_sample_does_not_select_its_device() {
        let mut off = s("2", "20", "B", 1.0, true);
        off.is_active = false;
        let list = [s("1", "10", "A", 1.0, true), off];
        assert_eq!(selected_device_hash(&list).as_deref(), Some("A"));
    }

    #[test]
    fn most_recent_sample_without_wav_does_not_select_its_device() {
        let list = [s("1", "10", "A", 1.0, true), s("2", "20", "", 0.0, false)];
        assert_eq!(selected_device_hash(&list).as_deref(), Some("A"));
    }

    #[test]
    fn negative_duration_does_not_inflate_the_budget() {
        let list = [s("1", "1", "A", -50.0, true), s("2", "2", "A", 60.0, true)];
        assert!(budget_for(&list, "A").used_seconds >= 60.0);
    }

    #[test]
    fn hostile_durations_do_not_open_the_gate() {
        let list = [s("1", "1", "A", f32::NEG_INFINITY, true)];
        assert!(budget_for(&list, "A").remaining_seconds <= MAX_SPEECH_SECONDS);
        let list = [
            s("1", "1", "A", f32::NAN, true),
            s("2", "2", "A", f32::INFINITY, true),
        ];
        let b = budget_for(&list, "A");
        assert!(b.used_seconds.is_finite() && b.remaining_seconds.is_finite());
    }

    #[test]
    fn fits_reject_non_finite_or_negative_durations() {
        let b = budget_used(0.0);
        for bad in [f32::NAN, -1.0, f32::NEG_INFINITY, f32::INFINITY] {
            assert!(!fits_manual(&b, bad), "manual {bad}");
            assert!(!fits_take(&b, bad), "take {bad}");
        }
    }

    #[test]
    fn empty_list_selects_no_device() {
        assert_eq!(selected_device_hash(&[]), None);
    }

    #[test]
    fn inactive_sample_does_not_count() {
        let mut off = s("1", "1", "A", 30.0, true);
        off.is_active = false;
        let b = budget_for(&[off, s("2", "2", "A", 20.0, true)], "A");
        assert!((b.used_seconds - 20.0).abs() < 1e-4);
    }

    #[test]
    fn other_device_samples_do_not_count() {
        let b = budget_for(&[s("1", "1", "B", 30.0, true)], "A");
        assert!(b.used_seconds.abs() < 1e-6);
        assert!((b.remaining_seconds - MAX_SPEECH_SECONDS).abs() < 1e-4);
    }
}
