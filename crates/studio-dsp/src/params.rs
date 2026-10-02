//! Preset parameter table.
//!
//! Every value below is a documented **starting point** to be tuned by ear, not a measured
//! optimum. `Off` has no entry: it bypasses the chain entirely.

use crate::compressor::CompressorParams;
use crate::deesser::DeEsserParams;
use crate::preset::Preset;

/// One bell filter of the fixed EQ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EqBand {
    pub freq_hz: f64,
    pub q: f64,
    /// Gain at the centre frequency in dB (negative cuts, positive boosts).
    pub gain_db: f64,
}

/// Everything a preset configures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresetParams {
    /// Bell around 250 Hz that cuts low-mid boxiness.
    pub low_mid_cut: EqBand,
    /// Bell around 4 kHz that adds presence.
    pub presence: EqBand,
    pub deesser: DeEsserParams,
    pub compressor: CompressorParams,
}

/// Light compression, mild low-mid cut, no presence boost, gentle de-esser.
pub const NATURAL: PresetParams = PresetParams {
    low_mid_cut: EqBand {
        freq_hz: 250.0,
        q: 0.9,
        gain_db: -1.0,
    },
    presence: EqBand {
        freq_hz: 4_000.0,
        q: 0.9,
        gain_db: 0.0,
    },
    deesser: DeEsserParams {
        threshold_ratio: 0.6,
        ratio: 2.0,
        max_reduction_db: 3.0,
    },
    compressor: CompressorParams {
        threshold_db: -24.0,
        ratio: 2.0,
        attack_ms: 15.0,
        release_ms: 150.0,
        makeup_db: 1.0,
    },
};

/// 3:1 compression and +2 dB of presence.
pub const PODCAST: PresetParams = PresetParams {
    low_mid_cut: EqBand {
        freq_hz: 250.0,
        q: 0.9,
        gain_db: -2.0,
    },
    presence: EqBand {
        freq_hz: 4_000.0,
        q: 0.9,
        gain_db: 2.0,
    },
    deesser: DeEsserParams {
        threshold_ratio: 0.5,
        ratio: 3.0,
        max_reduction_db: 6.0,
    },
    compressor: CompressorParams {
        threshold_db: -24.0,
        ratio: 3.0,
        attack_ms: 10.0,
        release_ms: 120.0,
        makeup_db: 2.0,
    },
};

/// 4:1 compression and the strongest de-esser.
pub const BROADCAST: PresetParams = PresetParams {
    low_mid_cut: EqBand {
        freq_hz: 250.0,
        q: 0.9,
        gain_db: -3.0,
    },
    presence: EqBand {
        freq_hz: 4_000.0,
        q: 0.9,
        gain_db: 2.0,
    },
    deesser: DeEsserParams {
        threshold_ratio: 0.4,
        ratio: 4.0,
        max_reduction_db: 9.0,
    },
    compressor: CompressorParams {
        threshold_db: -26.0,
        ratio: 4.0,
        attack_ms: 8.0,
        release_ms: 100.0,
        makeup_db: 3.0,
    },
};

/// Looks up a preset's parameters; `None` for `Off`.
pub const fn for_preset(preset: Preset) -> Option<&'static PresetParams> {
    match preset {
        Preset::Off => None,
        Preset::Natural => Some(&NATURAL),
        Preset::Podcast => Some(&PODCAST),
        Preset::Broadcast => Some(&BROADCAST),
    }
}

#[cfg(test)]
mod tests {
    use super::{BROADCAST, NATURAL, PODCAST, for_preset};
    use crate::preset::Preset;

    #[test]
    fn off_has_no_parameters() {
        assert!(for_preset(Preset::Off).is_none());
    }

    #[test]
    fn each_active_preset_maps_to_its_table_entry() {
        assert_eq!(for_preset(Preset::Natural), Some(&NATURAL));
        assert_eq!(for_preset(Preset::Podcast), Some(&PODCAST));
        assert_eq!(for_preset(Preset::Broadcast), Some(&BROADCAST));
    }

    #[test]
    fn presets_follow_the_spec_character() {
        assert!(NATURAL.presence.gain_db.abs() < f64::EPSILON);
        assert!((PODCAST.compressor.ratio - 3.0).abs() < f64::EPSILON);
        assert!((PODCAST.presence.gain_db - 2.0).abs() < f64::EPSILON);
        assert!((BROADCAST.compressor.ratio - 4.0).abs() < f64::EPSILON);
        let ratios = [NATURAL, PODCAST, BROADCAST].map(|params| params.compressor.ratio);
        assert!(ratios.is_sorted());
        let de_essing = [NATURAL, PODCAST, BROADCAST].map(|params| params.deesser.max_reduction_db);
        assert!(de_essing.is_sorted());
        assert!(de_essing[2] > de_essing[1] && de_essing[1] > de_essing[0]);
    }

    #[test]
    fn every_value_is_in_a_sane_range() {
        for params in [NATURAL, PODCAST, BROADCAST] {
            assert!((200.0..=300.0).contains(&params.low_mid_cut.freq_hz));
            assert!(params.low_mid_cut.gain_db <= 0.0);
            assert!((3_000.0..=5_000.0).contains(&params.presence.freq_hz));
            assert!(params.presence.gain_db >= 0.0);
            assert!((0.0..1.0).contains(&params.deesser.threshold_ratio));
            assert!(params.deesser.ratio >= 1.0 && params.deesser.max_reduction_db >= 0.0);
            assert!(params.compressor.ratio >= 1.0);
            assert!(params.compressor.threshold_db < 0.0);
            assert!(params.compressor.attack_ms > 0.0 && params.compressor.release_ms > 0.0);
        }
    }
}
