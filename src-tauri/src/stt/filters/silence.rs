//! Audio duration and silence detection filters.

/// Minimum sample count for 300 ms of 16 kHz mono PCM audio.
pub const MIN_DURATION_SAMPLES: usize = 4800;

/// Default RMS silence threshold below which an audio buffer is considered silent.
pub const DEFAULT_SILENCE_RMS_THRESHOLD: f32 = 0.005;

/// Returns true if the audio buffer duration is under 300 ms (< 4,800 samples at 16 kHz).
pub fn is_too_short(samples: &[f32]) -> bool {
    samples.len() < MIN_DURATION_SAMPLES
}

/// Calculates Root-Mean-Square (RMS) amplitude of audio samples.
pub fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / (samples.len() as f32)).sqrt()
}

/// Returns true if the RMS amplitude of the audio buffer is below the silence threshold.
pub fn is_too_silent(samples: &[f32], threshold: f32) -> bool {
    calculate_rms(samples) < threshold
}
