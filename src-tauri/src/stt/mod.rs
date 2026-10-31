//! Speech to text module.
//!
//! - `trait_def`: core `SttProvider` trait and shared types (slice 1.4).
//! - `chain`: fallback provider chain (slice 1.4).
//! - `local_adapter`: wraps `LocalWhisper` as an `SttProvider` (slice 1.4).
//! - `local_whisper`: raw whisper-rs engine (slice 1.2).
//! - `filters`: silence / hallucination filters (slice 1.3).

pub mod chain;
pub mod filters;
pub mod local_adapter;
pub mod local_whisper;
pub mod trait_def;
