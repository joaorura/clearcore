#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_const_for_fn,
    clippy::too_many_arguments
)]

use realtime_noise_contracts::{
    Discontinuity, FrameEnvelope, HOP_SAMPLES, RealtimeTransport, SAMPLE_RATE_HZ, TransportFull,
};

use crate::{
    AudioEndpointConfig, DEFRAMER_CAPACITY_SAMPLES, EndpointFormatConverter, FormatError,
    InputAccumulator, OutputDeframer, Resampler,
};

const CHUNK_STACK_LEN: usize = 1024;

/// Worker coordinating format conversion, framing, deframing, and bounded transport queues.
///
/// Bridges native hardware capture/render audio callbacks with the engine's canonical
/// 48 kHz mono 480-sample [`AudioFrame`] transport pipelines.
pub struct FormatAdapterWorker<InTransport, OutTransport> {
    pub input_transport: InTransport,
    pub output_transport: OutTransport,
    accumulator: InputAccumulator,
    deframer: OutputDeframer,
    input_config: AudioEndpointConfig,
    output_config: AudioEndpointConfig,
    input_resampler: Option<Box<dyn Resampler>>,
    output_resampler: Option<Box<dyn Resampler>>,
    sequence: u64,
    generation: u64,
    had_input_drop: bool,
}

impl<InTransport, OutTransport> FormatAdapterWorker<InTransport, OutTransport>
where
    InTransport: RealtimeTransport,
    OutTransport: RealtimeTransport,
{
    /// Creates a new `FormatAdapterWorker` for canonical 48 kHz endpoints without resampling.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError::UnsupportedSampleRate`] if either endpoint rate is not `48_000` Hz.
    /// Returns [`FormatError::UnsupportedChannels`] if either endpoint channel count is not 1 or 2.
    pub fn new(
        input_transport: InTransport,
        output_transport: OutTransport,
        input_config: AudioEndpointConfig,
        output_config: AudioEndpointConfig,
    ) -> Result<Self, FormatError> {
        Self::with_resamplers(
            input_transport,
            output_transport,
            input_config,
            output_config,
            None,
            None,
        )
    }

    /// Creates a new `FormatAdapterWorker` with optional explicit resamplers.
    ///
    /// Non-48kHz endpoints require an explicit resampler; failing to provide one will fail closed.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError::UnsupportedSampleRate`] if a non-48kHz endpoint is specified without an explicit resampler.
    /// Returns [`FormatError::UnsupportedChannels`] if channel count is unsupported.
    pub fn with_resamplers(
        input_transport: InTransport,
        output_transport: OutTransport,
        input_config: AudioEndpointConfig,
        output_config: AudioEndpointConfig,
        input_resampler: Option<Box<dyn Resampler>>,
        output_resampler: Option<Box<dyn Resampler>>,
    ) -> Result<Self, FormatError> {
        if input_config.channels == 0 || input_config.channels > 2 {
            return Err(FormatError::UnsupportedChannels(input_config.channels));
        }
        if output_config.channels == 0 || output_config.channels > 2 {
            return Err(FormatError::UnsupportedChannels(output_config.channels));
        }

        if input_config.sample_rate_hz != SAMPLE_RATE_HZ && input_resampler.is_none() {
            return Err(FormatError::UnsupportedSampleRate(
                input_config.sample_rate_hz,
            ));
        }
        if output_config.sample_rate_hz != SAMPLE_RATE_HZ && output_resampler.is_none() {
            return Err(FormatError::UnsupportedSampleRate(
                output_config.sample_rate_hz,
            ));
        }

        Ok(Self {
            input_transport,
            output_transport,
            accumulator: InputAccumulator::new(),
            deframer: OutputDeframer::new(),
            input_config,
            output_config,
            input_resampler,
            output_resampler,
            sequence: 0,
            generation: 1,
            had_input_drop: false,
        })
    }

    /// Processes native float32 input callback samples.
    ///
    /// Converts stereo to mono if needed, resamples if configured, accumulates into frames,
    /// and pushes complete 480-sample frames to the input transport queue.
    ///
    /// Returns the number of complete frames pushed to the transport queue.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] if buffer lengths or formats are incompatible.
    pub fn on_input_f32(
        &mut self,
        samples: &[f32],
        capture_monotonic_ns: u64,
    ) -> Result<usize, FormatError> {
        if self.input_config.channels == 2 {
            let mut mono_chunk = [0.0f32; CHUNK_STACK_LEN / 2];
            let mut offset = 0;
            while offset < samples.len() {
                let remaining = samples.len() - offset;
                let chunk_stereo_len = remaining.min(CHUNK_STACK_LEN);
                let chunk_mono_len = chunk_stereo_len / 2;

                EndpointFormatConverter::stereo_to_mono(
                    &samples[offset..offset + chunk_stereo_len],
                    &mut mono_chunk[..chunk_mono_len],
                )?;

                self.push_mono_input(&mono_chunk[..chunk_mono_len])?;
                offset += chunk_stereo_len;
            }
        } else {
            self.push_mono_input(samples)?;
        }

        let frames_pushed = self.drain_accumulator_to_transport(capture_monotonic_ns);
        Ok(frames_pushed)
    }

    /// Processes native PCM16 signed 16-bit integer input callback samples.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] if buffer lengths or formats are incompatible.
    pub fn on_input_pcm16(
        &mut self,
        samples: &[i16],
        capture_monotonic_ns: u64,
    ) -> Result<usize, FormatError> {
        let mut float_chunk = [0.0f32; CHUNK_STACK_LEN];
        let mut offset = 0;
        let mut total_frames_pushed = 0;

        while offset < samples.len() {
            let remaining = samples.len() - offset;
            let chunk_len = remaining.min(CHUNK_STACK_LEN);

            EndpointFormatConverter::pcm16_to_f32(
                &samples[offset..offset + chunk_len],
                &mut float_chunk[..chunk_len],
            )?;

            let pushed = self.on_input_f32(&float_chunk[..chunk_len], capture_monotonic_ns)?;
            total_frames_pushed += pushed;
            offset += chunk_len;
        }

        Ok(total_frames_pushed)
    }

    /// Fills the native float32 output callback slice.
    ///
    /// Pulls frames from the output transport queue into the deframer and provides
    /// the requested chunk size. On underrun, outputs digital silence.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] if buffer lengths or configurations mismatch.
    pub fn on_output_f32(&mut self, output: &mut [f32]) -> Result<(), FormatError> {
        self.pull_transport_to_deframer();

        if self.output_config.channels == 2 {
            let expected_mono = output.len() / 2;
            if output.len() != expected_mono * 2 {
                return Err(FormatError::BufferLengthMismatch {
                    expected: expected_mono * 2,
                    actual: output.len(),
                });
            }

            let mut mono_chunk = [0.0f32; CHUNK_STACK_LEN / 2];
            let mut out_offset = 0;
            while out_offset < output.len() {
                let remaining_stereo = output.len() - out_offset;
                let chunk_stereo_len = remaining_stereo.min(CHUNK_STACK_LEN);
                let chunk_mono_len = chunk_stereo_len / 2;

                self.fill_mono_output(&mut mono_chunk[..chunk_mono_len])?;
                EndpointFormatConverter::mono_to_stereo(
                    &mono_chunk[..chunk_mono_len],
                    &mut output[out_offset..out_offset + chunk_stereo_len],
                )?;
                out_offset += chunk_stereo_len;
            }
        } else {
            self.fill_mono_output(output)?;
        }

        Ok(())
    }

    fn fill_mono_output(&mut self, output: &mut [f32]) -> Result<(), FormatError> {
        if let Some(resampler) = &mut self.output_resampler {
            let mut deframer_48k = [0.0f32; CHUNK_STACK_LEN];
            let mut out_offset = 0;
            while out_offset < output.len() {
                let out_remaining = output.len() - out_offset;
                let out_chunk = out_remaining.min(CHUNK_STACK_LEN);
                let ratio =
                    f64::from(resampler.source_rate_hz()) / f64::from(resampler.target_rate_hz());
                #[allow(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss
                )]
                let needed_48k = (((out_chunk as f64) * ratio).ceil() as usize).max(1);
                let needed = needed_48k.min(CHUNK_STACK_LEN);
                self.deframer.fill_slice(&mut deframer_48k[..needed]);
                let written = resampler.process(
                    &deframer_48k[..needed],
                    &mut output[out_offset..out_offset + out_chunk],
                )?;
                if written == 0 {
                    output[out_offset..out_offset + out_chunk].fill(0.0);
                    out_offset += out_chunk;
                } else {
                    out_offset += written;
                }
            }
        } else {
            self.deframer.fill_slice(output);
        }
        Ok(())
    }

    /// Fills the native PCM16 output callback slice.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] if buffer lengths or configurations mismatch.
    pub fn on_output_pcm16(&mut self, output: &mut [i16]) -> Result<(), FormatError> {
        let mut float_chunk = [0.0f32; CHUNK_STACK_LEN];
        let mut out_offset = 0;
        while out_offset < output.len() {
            let remaining = output.len() - out_offset;
            let chunk_len = remaining.min(CHUNK_STACK_LEN);

            self.on_output_f32(&mut float_chunk[..chunk_len])?;
            EndpointFormatConverter::f32_to_pcm16(
                &float_chunk[..chunk_len],
                &mut output[out_offset..out_offset + chunk_len],
            )?;
            out_offset += chunk_len;
        }

        Ok(())
    }

    fn push_mono_input(&mut self, samples: &[f32]) -> Result<(), FormatError> {
        if let Some(resampler) = &mut self.input_resampler {
            let mut resample_out = [0.0f32; CHUNK_STACK_LEN];
            let mut offset = 0;
            while offset < samples.len() {
                let remaining = samples.len() - offset;
                let in_chunk = remaining.min(CHUNK_STACK_LEN / 3);
                let out_len =
                    resampler.process(&samples[offset..offset + in_chunk], &mut resample_out)?;
                self.accumulator.push_samples(&resample_out[..out_len]);
                offset += in_chunk;
            }
        } else {
            self.accumulator.push_samples(samples);
        }
        Ok(())
    }

    fn drain_accumulator_to_transport(&mut self, capture_monotonic_ns: u64) -> usize {
        let mut pushed = 0;
        while let Some(frame) = self.accumulator.pop_frame() {
            let mut discontinuity = Discontinuity::NONE;
            if self.had_input_drop {
                discontinuity |= Discontinuity::CAPTURE_DROP;
                self.had_input_drop = false;
            }

            self.sequence = self.sequence.wrapping_add(1);
            let envelope = FrameEnvelope {
                samples: frame,
                sequence: self.sequence,
                capture_monotonic_ns,
                generation: self.generation,
                discontinuity,
            };

            match self.input_transport.try_push(envelope) {
                Ok(()) => {
                    pushed += 1;
                }
                Err(TransportFull) => {
                    self.had_input_drop = true;
                }
            }
        }
        pushed
    }

    fn pull_transport_to_deframer(&mut self) {
        while self.deframer.available_samples() + HOP_SAMPLES <= DEFRAMER_CAPACITY_SAMPLES {
            if let Some(frame) = self.output_transport.try_pop() {
                let _ = self.deframer.push_frame(&frame.samples);
            } else {
                break;
            }
        }
    }

    /// Sets the active generation ID for outgoing frames.
    pub fn set_generation(&mut self, generation: u64) {
        self.generation = generation;
    }

    /// Returns the current generation ID.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns a reference to the internal input accumulator.
    #[must_use]
    pub const fn input_accumulator(&self) -> &InputAccumulator {
        &self.accumulator
    }

    /// Returns a reference to the internal output deframer.
    #[must_use]
    pub const fn output_deframer(&self) -> &OutputDeframer {
        &self.deframer
    }

    /// Flushes both input and output buffers.
    pub fn clear(&mut self) {
        self.accumulator.clear();
        self.deframer.clear();
        self.had_input_drop = false;
    }
}
