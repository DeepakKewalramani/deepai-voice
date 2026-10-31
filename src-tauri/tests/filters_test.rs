use deepai_voice_lib::stt::filters::{
    MIN_DURATION_SAMPLES, filter_fillers, is_hallucination, is_too_short, is_too_silent,
    sanitize_transcript,
};

#[test]
fn test_duration_filter_rejects_under_300ms() {
    let short_audio = vec![0.1f32; MIN_DURATION_SAMPLES - 1];
    assert!(is_too_short(&short_audio));

    let valid_audio = vec![0.1f32; MIN_DURATION_SAMPLES];
    assert!(!is_too_short(&valid_audio));
}

#[test]
fn test_silence_gate_detects_low_rms() {
    // Pure silence
    let silence = vec![0.0f32; 16000];
    assert!(is_too_silent(&silence, 0.005));

    // Very low background noise (RMS well below 0.005)
    let low_noise = vec![0.001f32; 16000];
    assert!(is_too_silent(&low_noise, 0.005));

    // Normal audible voice signal
    let voice: Vec<f32> = (0..16000).map(|i| ((i as f32) * 0.1).sin() * 0.1).collect();
    assert!(!is_too_silent(&voice, 0.005));
}

#[test]
fn test_hallucination_filter_known_patterns() {
    assert!(is_hallucination("[BLANK_AUDIO]"));
    assert!(is_hallucination("   [BLANK_AUDIO]   "));
    assert!(is_hallucination("(silence)"));
    assert!(is_hallucination("Thank you."));
    assert!(is_hallucination("Thank you very much."));
    assert!(is_hallucination("Thanks for watching!"));
    assert!(is_hallucination("Subtitles by the Amara.org community"));
    assert!(is_hallucination("Subtitles by"));

    // Legitimate sentences containing those words must NOT be flagged as hallucinations
    assert!(!is_hallucination(
        "Thank you for the review, let's ship this."
    ));
    assert!(!is_hallucination("The subtitles look great on the video."));
    assert!(!is_hallucination(
        "Please watch the recording of the meeting."
    ));
}

#[test]
fn test_hallucination_filter_repetition_loops() {
    // Repetitive loops generated when Whisper hallucinates
    let loop_chars = "........................................";
    assert!(is_hallucination(loop_chars));

    let loop_words = "bye bye bye bye bye bye bye bye bye bye";
    assert!(is_hallucination(loop_words));

    let valid_text = "Good bye and see you tomorrow!";
    assert!(!is_hallucination(valid_text));
}

#[test]
fn test_filler_filter_removes_standalone_fillers() {
    let input = "Um, I think, uh, we should go ahead, ah, with this.";
    let cleaned = filter_fillers(input);
    assert_eq!(cleaned, "I think we should go ahead with this.");

    let input2 = "hmm, let me check";
    let cleaned2 = filter_fillers(input2);
    assert_eq!(cleaned2, "Let me check");
}

#[test]
fn test_filler_filter_preserves_words_with_substrings() {
    let input = "The human with an umbrella spoke to Uhura in the summer.";
    let cleaned = filter_fillers(input);
    assert_eq!(
        cleaned,
        "The human with an umbrella spoke to Uhura in the summer."
    );
}

#[test]
fn test_sanitize_transcript_full_pipeline() {
    assert_eq!(sanitize_transcript("[BLANK_AUDIO]"), None);
    assert_eq!(sanitize_transcript("Thank you."), None);
    assert_eq!(sanitize_transcript("   "), None);

    let input = "Um, this is a real message.";
    let output = sanitize_transcript(input);
    assert_eq!(output, Some("This is a real message.".to_string()));
}

#[test]
fn test_sanitize_filler_only_returns_none() {
    assert_eq!(sanitize_transcript("Um."), None);
    assert_eq!(sanitize_transcript("Hmm."), None);
    assert_eq!(sanitize_transcript("Uh, um..."), None);
    assert_eq!(filter_fillers("Um."), "");
}

#[test]
fn test_filler_filter_keeps_hyphenated_words() {
    assert_eq!(filter_fillers("Uh-huh, that works"), "Uh-huh, that works");
    assert_eq!(filter_fillers("well, uh-huh"), "well, uh-huh");
    assert_eq!(filter_fillers("a er-ror b"), "a er-ror b");
}

#[test]
fn test_hallucination_punctuation_only_and_variants() {
    assert!(is_hallucination("."));
    assert!(is_hallucination("?"));
    assert!(is_hallucination("..."));
    assert!(is_hallucination("Thank you!"));
    assert!(is_hallucination("Thank you. Thank you. Thank you."));
    assert!(is_hallucination("Thanks for watching!!"));
    assert_eq!(sanitize_transcript("."), None);
}

#[test]
fn test_hallucination_word_loop_with_punctuation() {
    assert!(is_hallucination("Bye. Bye. bye bye bye"));
}

#[test]
fn test_legit_short_text_unchanged() {
    assert!(!is_hallucination("Thank you for the update"));
    assert_eq!(
        sanitize_transcript("Thank you for the update"),
        Some("Thank you for the update".to_string())
    );
}

#[test]
fn test_filler_filter_capitalizes_only_after_leading_filler_removed() {
    assert_eq!(filter_fillers("iPhone is great"), "iPhone is great");
    assert_eq!(filter_fillers("um, iPhone is great"), "IPhone is great");
}

#[test]
fn test_short_emphatic_repeat_is_kept() {
    assert!(!is_hallucination("No, no, no, no."));
    assert!(is_hallucination("no no no no no"));
}
