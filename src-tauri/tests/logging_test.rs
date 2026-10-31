use deepai_voice_lib::logging::{
    events::{AudioEvent, LlmEvent, PasteEvent, SttEvent},
    init_logging, redact_secrets,
};

#[test]
fn test_init_logging_idempotent() {
    // Should initialize successfully or return already initialized without panicking
    let res1 = init_logging();
    assert!(res1.is_ok());
    let res2 = init_logging();
    assert!(res2.is_ok());
}

#[test]
fn test_redact_anthropic_api_key() {
    let raw = "Sending request to Anthropic with key sk-ant-api03-abcdef1234567890_XYZ-more and model claude-haiku-5-5";
    let redacted = redact_secrets(raw);
    assert!(!redacted.contains("sk-ant-api03-"));
    assert!(!redacted.contains("abcdef1234567890"));
    assert!(redacted.contains("[REDACTED_KEY]"));
    assert!(redacted.contains("claude-haiku-5-5"));
}

#[test]
fn test_redact_generic_api_key() {
    let raw = "Configured provider with api_key sk-proj-1234567890abcdef1234567890 in registry";
    let redacted = redact_secrets(raw);
    assert!(!redacted.contains("sk-proj-"));
    assert!(!redacted.contains("1234567890abcdef"));
    assert!(redacted.contains("[REDACTED_KEY]"));
}

#[test]
fn test_redact_bearer_token() {
    let raw = "HTTP Header: Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.xyz";
    let redacted = redact_secrets(raw);
    assert!(!redacted.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    assert!(redacted.contains("Bearer [REDACTED_KEY]"));
}

#[test]
fn test_preserve_normal_text() {
    let raw = "Recording started for 120s with sample rate 16000Hz mono";
    let redacted = redact_secrets(raw);
    assert_eq!(raw, redacted);
}

#[test]
fn test_structured_audio_event_contains_no_raw_samples() {
    let event = AudioEvent {
        duration_ms: 5000,
        sample_count: 80000,
        rms: 0.042,
    };
    let formatted = event.to_log_string();
    assert!(formatted.contains("duration_ms=5000"));
    assert!(formatted.contains("sample_count=80000"));
    assert!(formatted.contains("rms=0.042"));
    // Verify no sample array or audio buffer text
    assert!(!formatted.contains("samples:"));
    assert!(!formatted.contains("buffer:"));
}

#[test]
fn test_structured_stt_event_contains_no_transcript_words() {
    let event = SttEvent {
        provider_id: "whisper-local".to_string(),
        duration_ms: 850,
        transcript_char_len: 42,
        success: true,
    };
    let formatted = event.to_log_string();
    assert!(formatted.contains("provider_id=whisper-local"));
    assert!(formatted.contains("duration_ms=850"));
    assert!(formatted.contains("char_len=42"));
    assert!(formatted.contains("success=true"));
    // Verify no transcript field or text content
    assert!(!formatted.contains("transcript:"));
    assert!(!formatted.contains("text:"));
}

#[test]
fn test_structured_llm_event_contains_no_prompt_or_completion_text() {
    let event = LlmEvent {
        provider_id: "anthropic-haiku".to_string(),
        duration_ms: 1200,
        input_char_len: 150,
        output_char_len: 142,
        success: true,
    };
    let formatted = event.to_log_string();
    assert!(formatted.contains("provider_id=anthropic-haiku"));
    assert!(formatted.contains("duration_ms=1200"));
    assert!(formatted.contains("in_chars=150"));
    assert!(formatted.contains("out_chars=142"));
    // Verify no prompt or completion text
    assert!(!formatted.contains("prompt:"));
    assert!(!formatted.contains("completion:"));
}

#[test]
fn test_structured_paste_event_contains_no_pasted_text() {
    let event = PasteEvent {
        duration_ms: 45,
        char_len: 142,
        target_app: "TextEdit".to_string(),
        success: true,
    };
    let formatted = event.to_log_string();
    assert!(formatted.contains("target_app=TextEdit"));
    assert!(formatted.contains("char_len=142"));
    assert!(formatted.contains("success=true"));
    assert!(!formatted.contains("pasted:"));
}
