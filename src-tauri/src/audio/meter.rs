//! Real-time audio level metering (RMS and Peak dB)

/// Calculates linear RMS (root mean square) of audio samples.
/// Returns 0.0 for empty slices.
pub fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Calculates RMS in dBFS (decibels relative to full scale).
/// Returns -100.0 dB floor for silent or empty audio.
pub fn calculate_rms_db(samples: &[f32]) -> f32 {
    let rms = calculate_rms(samples);
    linear_to_db(rms)
}

/// Calculates peak amplitude in dBFS.
/// Returns -100.0 dB floor for silent or empty audio.
pub fn calculate_peak_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return -100.0;
    }
    let peak = samples.iter().fold(0.0f32, |max, &s| max.max(s.abs()));
    linear_to_db(peak)
}

#[inline]
fn linear_to_db(val: f32) -> f32 {
    if val <= 1e-5 {
        -100.0
    } else {
        20.0 * val.log10()
    }
}
