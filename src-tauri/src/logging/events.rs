//! Structured pipeline events. Fields are metadata only: durations, counts,
//! lengths, ids and status. There is deliberately no field that can hold audio
//! samples, transcript text, prompts, completions or pasted text.

/// Audio capture finished.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioEvent {
    pub duration_ms: u64,
    pub sample_count: usize,
    pub rms: f32,
}

/// Speech-to-text call finished.
#[derive(Debug, Clone, PartialEq)]
pub struct SttEvent {
    pub provider_id: String,
    pub duration_ms: u64,
    pub transcript_char_len: usize,
    pub success: bool,
}

/// LLM cleanup call finished.
#[derive(Debug, Clone, PartialEq)]
pub struct LlmEvent {
    pub provider_id: String,
    pub duration_ms: u64,
    pub input_char_len: usize,
    pub output_char_len: usize,
    pub success: bool,
}

/// Paste into the target app finished.
#[derive(Debug, Clone, PartialEq)]
pub struct PasteEvent {
    pub duration_ms: u64,
    pub char_len: usize,
    pub target_app: String,
    pub success: bool,
}

impl AudioEvent {
    pub fn to_log_string(&self) -> String {
        format!(
            "event=audio duration_ms={} sample_count={} rms={}",
            self.duration_ms, self.sample_count, self.rms
        )
    }
    pub fn emit(&self) {
        tracing::info!("{}", self.to_log_string());
    }
}

impl SttEvent {
    pub fn to_log_string(&self) -> String {
        super::redact_secrets(&format!(
            "event=stt provider_id={} duration_ms={} char_len={} success={}",
            self.provider_id, self.duration_ms, self.transcript_char_len, self.success
        ))
    }
    pub fn emit(&self) {
        tracing::info!("{}", self.to_log_string());
    }
}

impl LlmEvent {
    pub fn to_log_string(&self) -> String {
        super::redact_secrets(&format!(
            "event=llm provider_id={} duration_ms={} in_chars={} out_chars={} success={}",
            self.provider_id,
            self.duration_ms,
            self.input_char_len,
            self.output_char_len,
            self.success
        ))
    }
    pub fn emit(&self) {
        tracing::info!("{}", self.to_log_string());
    }
}

impl PasteEvent {
    pub fn to_log_string(&self) -> String {
        super::redact_secrets(&format!(
            "event=paste target_app={} duration_ms={} char_len={} success={}",
            self.target_app, self.duration_ms, self.char_len, self.success
        ))
    }
    pub fn emit(&self) {
        tracing::info!("{}", self.to_log_string());
    }
}
