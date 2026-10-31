//! STT filtering stages: duration checks, silence gate, hallucination detection, and filler removal.

pub mod fillers;
pub mod hallucination;
pub mod silence;

pub use fillers::filter_fillers;
pub use hallucination::is_hallucination;
pub use silence::{
    DEFAULT_SILENCE_RMS_THRESHOLD, MIN_DURATION_SAMPLES, calculate_rms, is_too_short, is_too_silent,
};

/// Sanitizes a raw transcript by running hallucination detection and filler stripping.
/// Returns None if the transcript is a hallucination or empty.
pub fn sanitize_transcript(raw_text: &str) -> Option<String> {
    if is_hallucination(raw_text) {
        return None;
    }

    let cleaned = filter_fillers(raw_text);
    if cleaned.is_empty() || is_hallucination(&cleaned) {
        None
    } else {
        Some(cleaned)
    }
}
