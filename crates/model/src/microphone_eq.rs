#![allow(
    clippy::suboptimal_flops,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
//! Spectral microphone calibration and equalization module.
//!
//! This module analyzes audio buffers recorded during user enrollment or calibration,
//! computes the Long-Term Average Spectrum (LTAS) across the 32 ERB frequency bands
//! (48 kHz / 32 bands matching the `DeepFilterNet` / `spectral_eq` layout), compares the
//! observed spectrum with a reference Speech Target Curve, applies spectral smoothing
//! across adjacent bands, and derives strictly clamped [`BandGains`] in `[-6.0, +12.0]` dB
//! ready to be applied or embedded into a [`VoiceProfile`].

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::voice_profile::{
    BandGains, MAX_EQ_GAIN_DB, MIN_EQ_GAIN_DB, NUM_ERB_BANDS, VoiceProfile, VoiceProfileError,
};

/// Analysis sample rate in hertz (48 kHz, native to the model and STFT pipeline).
pub const CALIBRATION_SAMPLE_RATE_HZ: u32 = 48_000;
/// FFT window size (960 samples = 20 ms at 48 kHz).
pub const CALIBRATION_FFT_SIZE: usize = 960;
/// Analysis hop size (480 samples = 10 ms at 48 kHz, 50% overlap).
pub const CALIBRATION_HOP_SIZE: usize = 480;
/// Number of non-negative frequency bins for real FFT of size 960 (481 bins).
pub const CALIBRATION_N_FREQS: usize = CALIBRATION_FFT_SIZE / 2 + 1;

/// ERB band widths for the 32 bands at 48 kHz / 960 FFT (sum = 481 bins).
pub const ERB_BAND_WIDTHS: [usize; NUM_ERB_BANDS] = [
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 5, 5, 7, 7, 8, 10, 12, 13, 15, 18, 20, 24, 28, 31, 37,
    42, 50, 56, 67,
];

/// Index range of the core speech bands (approximately 200 Hz to 4,500 Hz).
const CORE_SPEECH_BAND_START: usize = 2;
const CORE_SPEECH_BAND_END: usize = 20;

/// Errors arising during microphone spectral calibration.
#[derive(Debug)]
pub enum MicrophoneEqError {
    EmptyAudio,
    TooShort {
        samples: usize,
        min_samples: usize,
    },
    InvalidSampleRate(u32),
    NonFiniteSample {
        index: usize,
    },
    TooQuiet {
        rms_dbfs: f32,
    },
    InsufficientSpeech {
        active_frames: usize,
        min_required: usize,
    },
    Profile(VoiceProfileError),
}

impl fmt::Display for MicrophoneEqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyAudio => write!(f, "calibration audio buffer is empty"),
            Self::TooShort {
                samples,
                min_samples,
            } => {
                write!(
                    f,
                    "calibration audio has {samples} samples, minimum required is {min_samples}"
                )
            }
            Self::InvalidSampleRate(sr) => write!(f, "invalid sample rate: {sr} Hz"),
            Self::NonFiniteSample { index } => {
                write!(f, "non-finite sample at index {index}")
            }
            Self::TooQuiet { rms_dbfs } => {
                write!(f, "calibration audio is too quiet ({rms_dbfs:.1} dBFS)")
            }
            Self::InsufficientSpeech {
                active_frames,
                min_required,
            } => {
                write!(
                    f,
                    "insufficient active speech frames: {active_frames} (minimum required {min_required})"
                )
            }
            Self::Profile(err) => write!(f, "profile error: {err}"),
        }
    }
}

impl std::error::Error for MicrophoneEqError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Profile(err) => Some(err),
            _ => None,
        }
    }
}

impl From<VoiceProfileError> for MicrophoneEqError {
    fn from(err: VoiceProfileError) -> Self {
        Self::Profile(err)
    }
}

/// Returns the center bin index (as `f32`) for each of the 32 ERB bands.
#[must_use]
pub fn erb_band_center_bins() -> [f32; NUM_ERB_BANDS] {
    let mut centers = [0.0_f32; NUM_ERB_BANDS];
    let mut start = 0_usize;
    for (center, &width) in centers.iter_mut().zip(&ERB_BAND_WIDTHS) {
        #[allow(clippy::cast_precision_loss)]
        let s = start as f32;
        #[allow(clippy::cast_precision_loss)]
        let w = width as f32;
        *center = s + (w - 1.0) / 2.0;
        start += width;
    }
    centers
}

/// Returns the center frequency in Hz for each of the 32 ERB bands at 48 kHz.
#[must_use]
pub fn erb_band_center_frequencies_hz() -> [f32; NUM_ERB_BANDS] {
    let mut freqs = [0.0_f32; NUM_ERB_BANDS];
    let centers = erb_band_center_bins();
    #[allow(clippy::cast_precision_loss)]
    let bin_hz = CALIBRATION_SAMPLE_RATE_HZ as f32 / CALIBRATION_FFT_SIZE as f32; // 50.0 Hz
    for (f, &c) in freqs.iter_mut().zip(&centers) {
        *f = c * bin_hz;
    }
    freqs
}

/// A target speech spectral envelope (Speech Target Curve) across 32 ERB bands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeechTargetCurve {
    pub levels_db: [f32; NUM_ERB_BANDS],
}

impl SpeechTargetCurve {
    /// Standard reference studio/broadcast speech LTAS curve across the 32 ERB bands.
    ///
    /// Based on standard acoustic speech spectra (Byrne et al. 1994, ANSI S3.5) normalized
    /// to 0 dB across core speech frequencies.
    #[must_use]
    pub const fn broadcast() -> Self {
        Self {
            levels_db: [
                -14.67, 3.21, 3.71, 4.21, 4.71, 4.16, 3.41, 2.77, 2.21, 1.71, 1.27, 0.86, 0.46,
                -0.39, -1.40, -2.30, -2.94, -3.35, -4.16, -6.79, -8.15, -9.45, -10.77, -12.08,
                -13.38, -15.50, -17.65, -19.78, -21.92, -24.07, -26.21, -28.34,
            ],
        }
    }

    /// Flat reference curve (0.0 dB across all bands), useful for measuring calibration against flat stimuli.
    #[must_use]
    pub const fn flat() -> Self {
        Self {
            levels_db: [0.0; NUM_ERB_BANDS],
        }
    }

    /// Constructs a target curve from an array of 32 decibel levels.
    pub fn from_array(levels_db: [f32; NUM_ERB_BANDS]) -> Result<Self, VoiceProfileError> {
        for (i, &lvl) in levels_db.iter().enumerate() {
            if !lvl.is_finite() {
                return Err(VoiceProfileError::NonFiniteValue {
                    field: "levels_db",
                    index: i,
                });
            }
        }
        Ok(Self { levels_db })
    }

    #[must_use]
    pub const fn levels(&self) -> &[f32; NUM_ERB_BANDS] {
        &self.levels_db
    }
}

impl Default for SpeechTargetCurve {
    fn default() -> Self {
        Self::broadcast()
    }
}

/// Configuration parameters for microphone spectral calibration estimation.
#[derive(Debug, Clone, PartialEq)]
pub struct MicrophoneEqConfig {
    /// Neighboring band smoothing weight in `[0.0, 0.45]`. Default is 0.25.
    pub smoothing_factor: f32,
    /// Number of smoothing passes across adjacent ERB bands. Default is 2.
    pub smoothing_passes: usize,
    /// Minimum active speech frames required to compute a valid LTAS. Default is 10 (100 ms).
    pub min_speech_frames: usize,
    /// Relative threshold (dB below maximum frame energy) to consider a frame speech. Default is 35.0 dB.
    pub vad_relative_threshold_db: f32,
    /// Minimum allowed gain in dB (>= -6.0 dB). Default is -6.0 dB.
    pub min_gain_db: f32,
    /// Maximum allowed gain in dB (<= 12.0 dB). Default is 12.0 dB.
    pub max_gain_db: f32,
    /// Whether to smoothly decay gains towards 0 dB for frequencies above the input Nyquist rate. Default is true.
    pub unobserved_high_band_decay: bool,
    /// Maximum boost allowed on band 0 (< 100 Hz rumble protection). Default is 0.0 dB.
    pub sub_bass_boost_limit_db: f32,
}

impl Default for MicrophoneEqConfig {
    fn default() -> Self {
        Self {
            smoothing_factor: 0.25,
            smoothing_passes: 2,
            min_speech_frames: 10,
            vad_relative_threshold_db: 35.0,
            min_gain_db: MIN_EQ_GAIN_DB,
            max_gain_db: MAX_EQ_GAIN_DB,
            unobserved_high_band_decay: true,
            sub_bass_boost_limit_db: 0.0,
        }
    }
}

/// Long-Term Average Spectrum (LTAS) measurements across the 32 ERB bands.
#[derive(Debug, Clone, PartialEq)]
pub struct LtasMeasurement {
    /// Measured average power spectral density in dB per ERB band.
    pub band_powers_db: [f32; NUM_ERB_BANDS],
    /// Number of active speech frames used in the measurement.
    pub active_speech_frames: usize,
    /// Total number of frames analyzed in the recording.
    pub total_frames: usize,
    /// Global RMS of the input signal in dBFS.
    pub mean_rms_dbfs: f32,
}

/// Computes the Long-Term Average Spectrum (LTAS) of audio across the 32 ERB bands.
pub fn compute_ltas(
    samples: &[f32],
    sample_rate: u32,
    config: &MicrophoneEqConfig,
) -> Result<LtasMeasurement, MicrophoneEqError> {
    if samples.is_empty() {
        return Err(MicrophoneEqError::EmptyAudio);
    }
    if sample_rate == 0 {
        return Err(MicrophoneEqError::InvalidSampleRate(0));
    }
    if let Some(index) = samples.iter().position(|s| !s.is_finite()) {
        return Err(MicrophoneEqError::NonFiniteSample { index });
    }

    // Resample to 48 kHz if input is at a different rate
    let audio_48k = if sample_rate == CALIBRATION_SAMPLE_RATE_HZ {
        samples.to_vec()
    } else {
        resample_to_48k(samples, sample_rate)?
    };

    if audio_48k.len() < CALIBRATION_FFT_SIZE {
        return Err(MicrophoneEqError::TooShort {
            samples: audio_48k.len(),
            min_samples: CALIBRATION_FFT_SIZE,
        });
    }

    // Overall RMS calculation
    let rms_sum: f64 = audio_48k.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    #[allow(clippy::cast_precision_loss)]
    let rms_mean = rms_sum / audio_48k.len() as f64;
    #[allow(clippy::cast_possible_truncation)]
    let mean_rms_dbfs = (10.0 * (rms_mean + 1e-12).log10()) as f32;

    if mean_rms_dbfs < -80.0 {
        return Err(MicrophoneEqError::TooQuiet {
            rms_dbfs: mean_rms_dbfs,
        });
    }

    // Framing and VAD
    let num_frames = (audio_48k.len() - CALIBRATION_FFT_SIZE) / CALIBRATION_HOP_SIZE + 1;
    let window = vorbis_window_960();

    let mut frame_energies = Vec::with_capacity(num_frames);
    for frame_idx in 0..num_frames {
        let start = frame_idx * CALIBRATION_HOP_SIZE;
        let slice = &audio_48k[start..start + CALIBRATION_FFT_SIZE];
        let mut e = 0.0_f32;
        for &s in slice {
            e += s * s;
        }
        frame_energies.push(e);
    }

    let max_energy = frame_energies.iter().copied().fold(0.0_f32, f32::max);
    if max_energy <= 1e-12 {
        return Err(MicrophoneEqError::TooQuiet {
            rms_dbfs: mean_rms_dbfs,
        });
    }

    let min_energy_ratio = 10.0_f32.powf(-config.vad_relative_threshold_db / 10.0);
    let energy_threshold = max_energy * min_energy_ratio;

    // Accumulate power spectrum for active speech frames
    let mut power_accum = [0.0_f64; CALIBRATION_N_FREQS];
    let mut active_frames = 0_usize;
    let mut frame_buf = [0.0_f32; CALIBRATION_FFT_SIZE];
    let mut spec_re = [0.0_f32; CALIBRATION_N_FREQS];
    let mut spec_im = [0.0_f32; CALIBRATION_N_FREQS];

    for (frame_idx, &energy) in frame_energies.iter().enumerate() {
        if energy < energy_threshold {
            continue;
        }
        active_frames += 1;
        let start = frame_idx * CALIBRATION_HOP_SIZE;
        let slice = &audio_48k[start..start + CALIBRATION_FFT_SIZE];

        for (b, (&s, &w)) in frame_buf.iter_mut().zip(slice.iter().zip(&window)) {
            *b = s * w;
        }

        rfft_960(&frame_buf, &mut spec_re, &mut spec_im);

        for k in 0..CALIBRATION_N_FREQS {
            let re = f64::from(spec_re[k]);
            let im = f64::from(spec_im[k]);
            power_accum[k] += re * re + im * im;
        }
    }

    if active_frames < config.min_speech_frames {
        return Err(MicrophoneEqError::InsufficientSpeech {
            active_frames,
            min_required: config.min_speech_frames,
        });
    }

    #[allow(clippy::cast_precision_loss)]
    let inv_frames = 1.0 / active_frames as f64;

    // Integrate power across the 32 ERB bands
    let mut band_powers_db = [0.0_f32; NUM_ERB_BANDS];
    let mut bin_start = 0_usize;
    for (b, &width) in ERB_BAND_WIDTHS.iter().enumerate() {
        let mut band_power_sum = 0.0_f64;
        for &p in &power_accum[bin_start..bin_start + width] {
            band_power_sum += p * inv_frames;
        }
        let avg_psd_per_bin = band_power_sum / width as f64;
        let db = (10.0 * (avg_psd_per_bin + 1e-12).log10()) as f32;
        band_powers_db[b] = db;
        bin_start += width;
    }

    Ok(LtasMeasurement {
        band_powers_db,
        active_speech_frames: active_frames,
        total_frames: num_frames,
        mean_rms_dbfs,
    })
}

/// Derives equalization [`BandGains`] from a precomputed [`LtasMeasurement`].
pub fn estimate_eq_from_ltas(
    ltas: &LtasMeasurement,
    target: &SpeechTargetCurve,
    config: &MicrophoneEqConfig,
    input_sample_rate: u32,
) -> Result<BandGains, MicrophoneEqError> {
    // 1. Calculate speech anchor means over core speech bands to normalize overall volume
    let mut obs_speech_sum = 0.0_f32;
    let mut target_speech_sum = 0.0_f32;
    let speech_band_count = (CORE_SPEECH_BAND_END - CORE_SPEECH_BAND_START + 1) as f32;

    for b in CORE_SPEECH_BAND_START..=CORE_SPEECH_BAND_END {
        obs_speech_sum += ltas.band_powers_db[b];
        target_speech_sum += target.levels_db[b];
    }

    let obs_mean = obs_speech_sum / speech_band_count;
    let target_mean = target_speech_sum / speech_band_count;

    // 2. Compute raw spectral deviation (target - observed) normalized by anchor
    let mut raw_gains = [0.0_f32; NUM_ERB_BANDS];
    for (b, g) in raw_gains.iter_mut().enumerate() {
        let norm_obs = ltas.band_powers_db[b] - obs_mean;
        let norm_target = target.levels_db[b] - target_mean;
        *g = norm_target - norm_obs;
    }

    // 3. Sub-bass protection (< 100 Hz rumble / handling noise suppression)
    if raw_gains[0] > config.sub_bass_boost_limit_db {
        raw_gains[0] = config.sub_bass_boost_limit_db;
    }

    // 4. Spectral smoothing across adjacent ERB bands
    let mut smoothed = raw_gains;
    let alpha = config.smoothing_factor.clamp(0.0, 0.45);
    for _ in 0..config.smoothing_passes {
        let mut next = smoothed;
        // Boundary band 0
        next[0] = (1.0 - alpha) * smoothed[0] + alpha * smoothed[1];
        // Interior bands
        for b in 1..(NUM_ERB_BANDS - 1) {
            next[b] =
                (1.0 - 2.0 * alpha) * smoothed[b] + alpha * (smoothed[b - 1] + smoothed[b + 1]);
        }
        // Boundary band 31
        next[NUM_ERB_BANDS - 1] =
            (1.0 - alpha) * smoothed[NUM_ERB_BANDS - 1] + alpha * smoothed[NUM_ERB_BANDS - 2];
        smoothed = next;
    }

    // 5. Unobserved frequency decay for sample rates lower than 48 kHz (e.g. 16 kHz enrollment)
    if config.unobserved_high_band_decay && input_sample_rate < CALIBRATION_SAMPLE_RATE_HZ {
        #[allow(clippy::cast_precision_loss)]
        let nyquist_hz = (input_sample_rate as f32) / 2.0;
        let trans_start_hz = nyquist_hz * 0.85;
        let center_freqs = erb_band_center_frequencies_hz();

        for (b, &fc) in center_freqs.iter().enumerate() {
            if fc >= nyquist_hz {
                smoothed[b] = 0.0;
            } else if fc > trans_start_hz {
                let factor = 0.5
                    * (1.0
                        + ((fc - trans_start_hz) / (nyquist_hz - trans_start_hz)
                            * std::f32::consts::PI)
                            .cos());
                smoothed[b] *= factor;
            }
        }
    }

    // 6. Strict gain clamping into [-6.0, +12.0] dB
    let min_limit = config.min_gain_db.max(MIN_EQ_GAIN_DB);
    let max_limit = config.max_gain_db.min(MAX_EQ_GAIN_DB);
    for g in &mut smoothed {
        *g = g.clamp(min_limit, max_limit);
    }

    let gains = BandGains::from_array(smoothed)?;
    Ok(gains)
}

/// Convenience function that computes LTAS and derives equalization [`BandGains`] in one step.
pub fn estimate_microphone_eq(
    samples: &[f32],
    sample_rate: u32,
    target: &SpeechTargetCurve,
    config: &MicrophoneEqConfig,
) -> Result<BandGains, MicrophoneEqError> {
    let ltas = compute_ltas(samples, sample_rate, config)?;
    estimate_eq_from_ltas(&ltas, target, config, sample_rate)
}

/// Embeds or updates the equalization gains inside a [`VoiceProfile`],
/// validating the gains and updating the profile's cryptographic integrity hash.
pub fn embed_eq_in_profile(
    profile: &mut VoiceProfile,
    eq: BandGains,
) -> Result<(), VoiceProfileError> {
    profile.set_eq(Some(eq))
}

// ---------------------------------------------------------------------------
// Internal DSP utilities (Vorbis Window, FFT 960, and Resampler)
// ---------------------------------------------------------------------------

/// Vorbis analysis window for 960 samples, matching `deep_filter`'s STFT.
fn vorbis_window_960() -> [f32; CALIBRATION_FFT_SIZE] {
    let mut window = [0.0_f32; CALIBRATION_FFT_SIZE];
    let pi = std::f64::consts::PI;
    let window_size_h = (CALIBRATION_FFT_SIZE / 2) as f64; // 480.0
    for (i, w) in window.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let sin = (0.5 * pi * (i as f64 + 0.5) / window_size_h).sin();
        #[allow(clippy::cast_possible_truncation)]
        let val = (0.5 * pi * sin * sin).sin() as f32;
        *w = val;
    }
    window
}

/// Mixed-radix Real FFT for 960 points (64 x 15 Cooley-Tukey decomposition).
///
/// Pure Rust, no heap allocations, deterministic and bit-exact within floating-point precision.
fn rfft_960(
    input: &[f32; CALIBRATION_FFT_SIZE],
    out_re: &mut [f32; CALIBRATION_N_FREQS],
    out_im: &mut [f32; CALIBRATION_N_FREQS],
) {
    // Step 1: 15 FFTs of size 64
    let mut step1_re = [[0.0_f32; 64]; 15];
    let mut step1_im = [[0.0_f32; 64]; 15];

    let mut buf_re = [0.0_f32; 64];
    let mut buf_im = [0.0_f32; 64];

    for n2 in 0..15 {
        for n1 in 0..64 {
            buf_re[n1] = input[15 * n1 + n2];
            buf_im[n1] = 0.0;
        }
        fft_64(&mut buf_re, &mut buf_im);
        step1_re[n2] = buf_re;
        step1_im[n2] = buf_im;
    }

    // Step 2 & 3: Multiply twiddles W_960^(k1 * n2) and do 64 FFTs of size 15 along n2
    let pi = std::f64::consts::PI;
    let mut col_re = [0.0_f32; 15];
    let mut col_im = [0.0_f32; 15];
    let mut fft15_re = [0.0_f32; 15];
    let mut fft15_im = [0.0_f32; 15];

    for k1 in 0..64 {
        for n2 in 0..15 {
            #[allow(clippy::cast_precision_loss)]
            let angle = -2.0 * pi * (k1 as f64) * (n2 as f64) / 960.0;
            #[allow(clippy::cast_possible_truncation)]
            let tw_cos = angle.cos() as f32;
            #[allow(clippy::cast_possible_truncation)]
            let tw_sin = angle.sin() as f32;

            let re = step1_re[n2][k1];
            let im = step1_im[n2][k1];

            // Complex multiply: (re + j im) * (cos + j sin)
            col_re[n2] = re * tw_cos - im * tw_sin;
            col_im[n2] = re * tw_sin + im * tw_cos;
        }

        // Direct size-15 DFT
        dft_15(&col_re, &col_im, &mut fft15_re, &mut fft15_im);

        // Step 4: Map to output frequencies k = k1 + 64 * k2
        for k2 in 0..15 {
            let k = k1 + 64 * k2;
            if k < CALIBRATION_N_FREQS {
                out_re[k] = fft15_re[k2];
                out_im[k] = fft15_im[k2];
            }
        }
    }
}

/// Radix-2 in-place Cooley-Tukey FFT of size 64.
fn fft_64(re: &mut [f32; 64], im: &mut [f32; 64]) {
    // Bit-reversal permutation (6 bits)
    for i in 0..64_usize {
        let rev = i.reverse_bits() >> (usize::BITS - 6);
        if i < rev {
            re.swap(i, rev);
            im.swap(i, rev);
        }
    }

    let pi = std::f64::consts::PI;
    let mut len = 2_usize;
    while len <= 64 {
        let half = len / 2;
        #[allow(clippy::cast_precision_loss)]
        let angle_step = -2.0 * pi / len as f64;
        let mut w_re = 1.0_f64;
        let mut w_im = 0.0_f64;
        let cos_step = angle_step.cos();
        let sin_step = angle_step.sin();

        for j in 0..half {
            #[allow(clippy::cast_possible_truncation)]
            let u_re = w_re as f32;
            #[allow(clippy::cast_possible_truncation)]
            let u_im = w_im as f32;

            let mut i = j;
            while i < 64 {
                let match_idx = i + half;
                let v_re = re[match_idx] * u_re - im[match_idx] * u_im;
                let v_im = re[match_idx] * u_im + im[match_idx] * u_re;

                re[match_idx] = re[i] - v_re;
                im[match_idx] = im[i] - v_im;
                re[i] += v_re;
                im[i] += v_im;

                i += len;
            }

            let next_w_re = w_re * cos_step - w_im * sin_step;
            let next_w_im = w_re * sin_step + w_im * cos_step;
            w_re = next_w_re;
            w_im = next_w_im;
        }
        len *= 2;
    }
}

/// Direct DFT of size 15.
fn dft_15(in_re: &[f32; 15], in_im: &[f32; 15], out_re: &mut [f32; 15], out_im: &mut [f32; 15]) {
    let pi = std::f64::consts::PI;
    for k in 0..15 {
        let mut sum_re = 0.0_f64;
        let mut sum_im = 0.0_f64;
        for n in 0..15 {
            #[allow(clippy::cast_precision_loss)]
            let angle = -2.0 * pi * (k as f64) * (n as f64) / 15.0;
            let cos = angle.cos();
            let sin = angle.sin();
            let r = f64::from(in_re[n]);
            let i = f64::from(in_im[n]);
            sum_re += r * cos - i * sin;
            sum_im += r * sin + i * cos;
        }
        #[allow(clippy::cast_possible_truncation)]
        {
            out_re[k] = sum_re as f32;
            out_im[k] = sum_im as f32;
        }
    }
}

/// Resamples an arbitrary audio buffer to 48 kHz.
///
/// Uses an exact polyphase 3x upsampler for 16 kHz input, and windowed sinc interpolation
/// for arbitrary sampling rates.
fn resample_to_48k(samples: &[f32], from_rate: u32) -> Result<Vec<f32>, MicrophoneEqError> {
    if from_rate == 0 {
        return Err(MicrophoneEqError::InvalidSampleRate(0));
    }
    if from_rate == CALIBRATION_SAMPLE_RATE_HZ {
        return Ok(samples.to_vec());
    }

    if from_rate == 16_000 {
        // High quality polyphase 3x upsampling (16 kHz -> 48 kHz)
        return Ok(upsample_16k_to_48k_polyphase(samples));
    }

    // General windowed-sinc resampler
    let ratio = f64::from(CALIBRATION_SAMPLE_RATE_HZ) / f64::from(from_rate);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let out_len = (samples.len() as f64 * ratio).round() as usize;
    let mut output = Vec::with_capacity(out_len);

    let filter_radius = 8_isize;
    let cutoff = if ratio < 1.0 { ratio * 0.95 } else { 0.95 };
    let pi = std::f64::consts::PI;

    for i in 0..out_len {
        let t_in = i as f64 / ratio;
        let center = t_in.floor() as isize;
        let mut acc = 0.0_f64;
        let mut weight_sum = 0.0_f64;

        for offset in -filter_radius..=filter_radius {
            let idx = center + offset;
            if idx >= 0 && (idx as usize) < samples.len() {
                let dx = t_in - idx as f64;
                let x = dx * cutoff;
                let sinc = if x.abs() < 1e-6 {
                    1.0
                } else {
                    (pi * x).sin() / (pi * x)
                };
                #[allow(clippy::cast_precision_loss)]
                let window_arg = (pi * dx / filter_radius as f64).cos();
                let window = 0.5 * (1.0 + window_arg);
                let w = sinc * window;
                acc += f64::from(samples[idx as usize]) * w;
                weight_sum += w;
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        let val = if weight_sum.abs() > 1e-6 {
            (acc / weight_sum) as f32
        } else {
            0.0_f32
        };
        output.push(val);
    }

    Ok(output)
}

/// Exact 3x polyphase upsampler from 16 kHz to 48 kHz.
fn upsample_16k_to_48k_polyphase(samples: &[f32]) -> Vec<f32> {
    // 45-tap low-pass FIR (15 taps per phase) with Blackman-Harris window, cutoff at 7.5 kHz
    const N_TAPS: usize = 45;
    const BRANCH_LEN: usize = N_TAPS / 3;

    // Precomputed polyphase branch impulse responses (fc = 7500 / 48000 = 0.15625)
    let mut h = [0.0_f64; N_TAPS];
    let fc = 7500.0 / 48000.0;
    let pi = std::f64::consts::PI;
    for (i, val) in h.iter_mut().enumerate() {
        let n = i as f64 - (N_TAPS - 1) as f64 / 2.0;
        let sinc = if n.abs() < 1e-9 {
            1.0
        } else {
            (2.0 * pi * fc * n).sin() / (pi * n)
        };
        let a = 2.0 * pi * (i as f64) / (N_TAPS - 1) as f64;
        let w = 0.35875 - 0.48829 * a.cos() + 0.14128 * (2.0 * a).cos() - 0.01168 * (3.0 * a).cos();
        *val = 3.0 * sinc * w;
    }

    let mut h0 = [0.0_f32; BRANCH_LEN];
    let mut h1 = [0.0_f32; BRANCH_LEN];
    let mut h2 = [0.0_f32; BRANCH_LEN];

    for k in 0..BRANCH_LEN {
        #[allow(clippy::cast_possible_truncation)]
        {
            h0[k] = h[3 * k] as f32;
            h1[k] = h[3 * k + 1] as f32;
            h2[k] = h[3 * k + 2] as f32;
        }
    }

    let mut out = vec![0.0_f32; samples.len() * 3];

    for i in 0..samples.len() {
        for k in 0..BRANCH_LEN {
            if i >= k {
                let past = samples[i - k];
                out[3 * i] += past * h0[k];
                out[3 * i + 1] += past * h1[k];
                out[3 * i + 2] += past * h2[k];
            }
        }
    }

    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_erb_frequencies_monotonicity() {
        let freqs = erb_band_center_frequencies_hz();
        assert_eq!(freqs.len(), NUM_ERB_BANDS);
        assert!(freqs[0] > 0.0 && freqs[0] < 100.0);
        assert!(freqs[NUM_ERB_BANDS - 1] > 20_000.0);
        for w in freqs.windows(2) {
            assert!(w[1] > w[0], "frequencies must be strictly increasing");
        }
    }

    #[test]
    fn test_speech_target_curves_validity() {
        let broadcast = SpeechTargetCurve::broadcast();
        assert_eq!(broadcast.levels_db.len(), NUM_ERB_BANDS);
        assert!(broadcast.levels_db.iter().all(|l| l.is_finite()));

        let flat = SpeechTargetCurve::flat();
        assert!(flat.levels_db.iter().all(|l| *l == 0.0));
    }

    #[test]
    fn test_rfft_960_against_pure_tones() {
        let mut signal = [0.0_f32; CALIBRATION_FFT_SIZE];
        let freq_hz = 1000.0_f32; // Bin 20 at 50 Hz/bin
        let pi = std::f32::consts::PI;
        for (n, s) in signal.iter_mut().enumerate() {
            *s = (2.0 * pi * freq_hz * (n as f32) / 48000.0).sin();
        }

        let mut spec_re = [0.0_f32; CALIBRATION_N_FREQS];
        let mut spec_im = [0.0_f32; CALIBRATION_N_FREQS];
        rfft_960(&signal, &mut spec_re, &mut spec_im);

        let mut powers = [0.0_f32; CALIBRATION_N_FREQS];
        for k in 0..CALIBRATION_N_FREQS {
            powers[k] = spec_re[k] * spec_re[k] + spec_im[k] * spec_im[k];
        }

        // Peak must occur at bin 20
        let (peak_bin, _) = powers
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        assert_eq!(peak_bin, 20, "1 kHz tone must peak at bin 20");
    }

    #[test]
    fn test_profile_embed_eq() {
        let mut profile =
            VoiceProfile::identity("mic-test", "Tester", "2026-10-05T00:00:00Z").unwrap();
        let initial_hash = profile.integrity_hash.clone();

        let mut raw = [0.0_f32; NUM_ERB_BANDS];
        raw[5] = 2.0;
        raw[20] = -1.5;
        let gains = BandGains::from_array(raw).unwrap();

        embed_eq_in_profile(&mut profile, gains.clone()).unwrap();
        assert_eq!(profile.eq.as_ref(), Some(&gains));
        assert_ne!(profile.integrity_hash, initial_hash);
        profile.verify_integrity().expect("must verify");
    }
}
