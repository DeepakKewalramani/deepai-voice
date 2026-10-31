pub mod capture;
pub mod meter;
pub mod resample;

#[cfg(target_os = "macos")]
pub use capture::AvFoundationProvider;
pub use capture::{
    AudioCaptureConfig, AudioCaptureSession, CpalDeviceProbe, InMemoryAudioBuffer, LevelCallback,
    LevelMeterEvent, MicPermissionProvider, MicPermissionStatus, RawChunkSink,
    check_microphone_permission, check_microphone_permission_with, map_av_authorization_status,
    request_microphone_permission, request_microphone_permission_with,
};
pub use meter::{calculate_peak_db, calculate_rms, calculate_rms_db};
pub use resample::{downmix_to_mono, resample_to_16k};
