#![forbid(unsafe_code)]

pub mod endpoint;
pub mod formats;
pub mod wasapi;

pub use endpoint::{
    DEFAULT_DEVICE_PATH, IOCTL_REALTIME_NOISE_ACQUIRE_SESSION, IOCTL_REALTIME_NOISE_GET_STATS,
    IOCTL_REALTIME_NOISE_RELEASE_SESSION, IOCTL_REALTIME_NOISE_SUBMIT_ENVELOPE, SupervisorState,
    TransportStats, WindowsVirtualMicrophone,
};
pub use formats::{
    AudioEndpointFormat, audio_frame_to_f32_bytes, audio_frame_to_pcm16_bytes,
    canonical_f32_to_pcm16, f32_bytes_to_audio_frame, pcm16_bytes_to_audio_frame,
    pcm16_to_canonical_f32, validate_format,
};
pub use wasapi::{WasapiAudioBackend, WasapiDeviceInfo};
