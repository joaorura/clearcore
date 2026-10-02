//! Maps the 32 ERB band gains of a [`BandGains`] onto the per-bin linear factors applied to the
//! enhanced spectrum right before the inverse STFT (`DfTract::set_spectral_eq_factors`).
//!
//! The hook runs inside the model's own STFT/iSTFT pair, so it adds zero algorithmic latency.

use crate::voice_profile::{BandGains, NUM_ERB_BANDS, VoiceProfileError};

/// Per-bin linear factors for `band_widths.iter().sum()` frequency bins.
///
/// `band_widths[k]` is the number of FFT bins of ERB band `k` (as in `DFState::erb`). Gains are
/// interpolated linearly **in dB** between band centers, so adjacent bands do not produce a step
/// at the band edge; bins below the first center and above the last one keep that band's gain.
/// A flat set of gains yields exactly the same factor for every bin.
pub fn bin_factors(
    gains: &BandGains,
    band_widths: &[usize],
) -> Result<Vec<f32>, VoiceProfileError> {
    gains.validate()?;
    if band_widths.len() != NUM_ERB_BANDS {
        return Err(VoiceProfileError::DimensionMismatch {
            field: "band_widths",
            expected: NUM_ERB_BANDS,
            actual: band_widths.len(),
        });
    }
    if band_widths.contains(&0) {
        return Err(VoiceProfileError::ValueOutOfRange {
            field: "band_widths",
            value: 0.0,
            min: 1.0,
            max: f32::MAX,
        });
    }
    let total: usize = band_widths.iter().sum();
    let mut centers = [0.0_f32; NUM_ERB_BANDS];
    let mut start = 0_usize;
    for (center, &width) in centers.iter_mut().zip(band_widths) {
        *center = bin_to_f32(start) + (bin_to_f32(width) - 1.0) / 2.0;
        start += width;
    }

    let mut factors = Vec::with_capacity(total);
    let mut band = 0_usize; // largest k with centers[k] <= bin
    for bin in 0..total {
        let position = bin_to_f32(bin);
        while band + 1 < NUM_ERB_BANDS && centers[band + 1] <= position {
            band += 1;
        }
        let db = if position <= centers[0] || band + 1 == NUM_ERB_BANDS {
            gains.gains_db[band]
        } else {
            let (low, high) = (gains.gains_db[band], gains.gains_db[band + 1]);
            let t = (position - centers[band]) / (centers[band + 1] - centers[band]);
            (high - low).mul_add(t, low)
        };
        factors.push(10.0_f32.powf(db / 20.0));
    }
    Ok(factors)
}

#[allow(clippy::cast_precision_loss)]
const fn bin_to_f32(value: usize) -> f32 {
    value as f32
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// ERB layout of the 48 kHz / N=960 / 32-band model (481 bins), equal to `DfTract::df_states[0].erb` (asserted in `tests/parity_eq.rs`).
    fn widths() -> Vec<usize> {
        vec![
            2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 5, 5, 7, 7, 8, 10, 12, 13, 15, 18, 20, 24, 28,
            31, 37, 42, 50, 56, 67,
        ]
    }

    fn gains_with(f: impl Fn(usize) -> f32) -> BandGains {
        let mut db = [0.0_f32; NUM_ERB_BANDS];
        for (index, value) in db.iter_mut().enumerate() {
            *value = f(index);
        }
        BandGains::from_array(db).expect("valid gains")
    }

    #[test]
    fn neutral_gains_give_unit_factors_for_every_bin() {
        let widths = widths();
        let factors = bin_factors(&BandGains::neutral(), &widths).expect("factors");
        assert_eq!(factors.len(), widths.iter().sum::<usize>());
        assert!(factors.iter().all(|f| f.to_bits() == 1.0_f32.to_bits()));
    }

    #[test]
    fn flat_gain_gives_exactly_the_same_factor_for_every_bin() {
        let factors = bin_factors(&gains_with(|_| 6.0), &widths()).expect("factors");
        let expected = 10.0_f32.powf(6.0 / 20.0);
        assert!(factors.iter().all(|f| f.to_bits() == expected.to_bits()));
    }

    #[test]
    fn step_between_bands_is_smoothed_and_monotonic() {
        let widths = widths();
        let factors = bin_factors(&gains_with(|k| if k < 16 { 0.0 } else { 12.0 }), &widths)
            .expect("factors");
        let db: Vec<f32> = factors.iter().map(|f| 20.0 * f.log10()).collect();
        assert!(
            db.windows(2).all(|w| w[1] >= w[0] - 1e-4),
            "must be non-decreasing"
        );
        let first_edge: usize = widths[..16].iter().sum();
        let max_step = db.windows(2).map(|w| w[1] - w[0]).fold(0.0_f32, f32::max);
        assert!(
            max_step < 12.0,
            "no 12 dB cliff between adjacent bins (max step {max_step})"
        );
        assert!(
            db[first_edge - 1] > 0.0 && db[first_edge] < 12.0,
            "the edge sits mid-transition"
        );
        assert!((db[0]).abs() < 1e-4 && (db[db.len() - 1] - 12.0).abs() < 1e-4);
    }

    #[test]
    fn band_center_bins_keep_the_band_gain() {
        let widths = widths();
        let gains = gains_with(|k| if k % 2 == 0 { -3.0 } else { 5.0 });
        let factors = bin_factors(&gains, &widths).expect("factors");
        let mut start = 0;
        for (k, &w) in widths.iter().enumerate() {
            if w % 2 == 1 {
                let center = start + w / 2;
                let db = 20.0 * factors[center].log10();
                assert!((db - gains.gains_db[k]).abs() < 1e-3, "band {k}: {db}");
            }
            start += w;
        }
    }

    #[test]
    fn rejects_wrong_band_count_and_empty_bands() {
        let gains = BandGains::neutral();
        assert!(matches!(
            bin_factors(&gains, &[1, 2, 3]),
            Err(VoiceProfileError::DimensionMismatch {
                field: "band_widths",
                ..
            })
        ));
        let mut widths = widths();
        widths[3] = 0;
        assert!(bin_factors(&gains, &widths).is_err());
    }

    #[test]
    fn rejects_out_of_range_gains() {
        let gains = BandGains {
            gains_db: [20.0; NUM_ERB_BANDS],
        };
        assert!(bin_factors(&gains, &widths()).is_err());
    }
}
