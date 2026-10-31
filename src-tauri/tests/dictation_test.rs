// Tests for the dictation orchestrator. Pure logic and fakes only: no mic,
// no Whisper model, no paste. Transcript text here is fixed test data.

use deepai_voice_lib::audio::MicPermissionStatus;
use deepai_voice_lib::dictation::state::{DictationState as S, Effect, Input, Machine};
use deepai_voice_lib::dictation::{
    MicGate, Outcome, drive, mic_gate, paste_error_status, process_audio, resolve_model_path,
};
use deepai_voice_lib::hotkey::HotkeyAction;
use deepai_voice_lib::paste::PasteError;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;

fn hk(a: HotkeyAction) -> Input {
    Input::Hotkey(a)
}

#[test]
fn start_from_idle_records() {
    let mut m = Machine::new();
    assert_eq!(
        m.handle(hk(HotkeyAction::StartRecording)),
        Some(Effect::StartCapture)
    );
    assert_eq!(*m.state(), S::Recording);
    assert_eq!(m.status_text(), "REC");
}

#[test]
fn stop_transcribes_and_discard_returns_to_idle() {
    let mut m = Machine::new();
    m.handle(hk(HotkeyAction::StartRecording));
    assert_eq!(
        m.handle(hk(HotkeyAction::StopRecording)),
        Some(Effect::StopAndTranscribe)
    );
    assert_eq!(*m.state(), S::Transcribing);
    assert_eq!(m.status_text(), "...");

    let mut m = Machine::new();
    m.handle(hk(HotkeyAction::StartRecording));
    assert_eq!(
        m.handle(hk(HotkeyAction::DiscardRecording)),
        Some(Effect::DiscardCapture)
    );
    assert_eq!(*m.state(), S::Idle);
    assert_eq!(m.status_text(), "");
}

#[test]
fn stop_in_idle_is_ignored() {
    let mut m = Machine::new();
    assert_eq!(m.handle(hk(HotkeyAction::StopRecording)), None);
    assert_eq!(m.handle(hk(HotkeyAction::None)), None);
    assert_eq!(*m.state(), S::Idle);
}

#[test]
fn start_while_busy_is_ignored() {
    let mut m = Machine::new();
    m.handle(hk(HotkeyAction::StartRecording));
    m.handle(hk(HotkeyAction::StopRecording));
    assert_eq!(m.handle(hk(HotkeyAction::StartRecording)), None);
    assert_eq!(*m.state(), S::Transcribing);
    m.handle(Input::Transcribed(Outcome::Text("hello".into())));
    assert_eq!(m.handle(hk(HotkeyAction::StartRecording)), None);
    assert_eq!(*m.state(), S::Pasting);
}

#[test]
fn text_goes_to_paste_then_idle() {
    let mut m = Machine::new();
    m.handle(hk(HotkeyAction::StartRecording));
    m.handle(hk(HotkeyAction::StopRecording));
    assert_eq!(
        m.handle(Input::Transcribed(Outcome::Text("hello".into()))),
        Some(Effect::Paste("hello".into()))
    );
    assert_eq!(*m.state(), S::Pasting);
    assert_eq!(m.handle(Input::Pasted(Ok(()))), None);
    assert_eq!(*m.state(), S::Idle);
}

#[test]
fn nothing_to_paste_returns_to_idle() {
    let mut m = Machine::new();
    m.handle(hk(HotkeyAction::StartRecording));
    m.handle(hk(HotkeyAction::StopRecording));
    assert_eq!(m.handle(Input::Transcribed(Outcome::Nothing)), None);
    assert_eq!(*m.state(), S::Idle);
}

#[test]
fn transcribe_failure_sets_error_and_next_press_recovers() {
    let mut m = Machine::new();
    m.handle(hk(HotkeyAction::StartRecording));
    m.handle(hk(HotkeyAction::StopRecording));
    m.handle(Input::Transcribed(Outcome::Failed("Model missing".into())));
    assert_eq!(*m.state(), S::Error("Model missing".into()));
    assert_eq!(m.status_text(), "Model missing");
    assert_eq!(
        m.handle(hk(HotkeyAction::StartRecording)),
        Some(Effect::StartCapture)
    );
    assert_eq!(*m.state(), S::Recording);
}

#[test]
fn paste_failure_sets_error() {
    let mut m = Machine::new();
    m.handle(hk(HotkeyAction::StartRecording));
    m.handle(hk(HotkeyAction::StopRecording));
    m.handle(Input::Transcribed(Outcome::Text("x".into())));
    m.handle(Input::Pasted(Err("Copied: paste blocked".into())));
    assert_eq!(*m.state(), S::Error("Copied: paste blocked".into()));
}

#[test]
fn start_failure_shows_error_even_after_quick_tap() {
    let mut m = Machine::new();
    m.handle(hk(HotkeyAction::StartRecording));
    m.handle(hk(HotkeyAction::DiscardRecording));
    m.handle(Input::StartFailed("Allow mic".into()));
    assert_eq!(*m.state(), S::Error("Allow mic".into()));
}

#[test]
fn clear_error_only_clears_error() {
    let mut m = Machine::new();
    m.handle(hk(HotkeyAction::StartRecording));
    m.handle(Input::ClearError);
    assert_eq!(*m.state(), S::Recording);
    m.handle(Input::StartFailed("e".into()));
    m.handle(Input::ClearError);
    assert_eq!(*m.state(), S::Idle);
}

#[test]
fn model_path_lookup_order() {
    let home = Some(PathBuf::from("/home/u"));
    let lib =
        PathBuf::from("/home/u/Library/Application Support/DeepAI Voice/models/ggml-base.en.bin");
    let rel = PathBuf::from("models/ggml-base.en.bin");

    // Env wins when set and present.
    let env = Some("/tmp/m.bin".to_string());
    let all = |_: &Path| true;
    assert_eq!(
        resolve_model_path(env.clone(), home.clone(), all),
        Some(PathBuf::from("/tmp/m.bin"))
    );
    // Env set but missing falls through to the app support dir.
    let only_lib = |p: &Path| p == lib;
    assert_eq!(
        resolve_model_path(env, home.clone(), only_lib),
        Some(lib.clone())
    );
    // Then the relative path.
    let only_rel = |p: &Path| p == rel;
    assert_eq!(resolve_model_path(None, home.clone(), only_rel), Some(rel));
    // Nothing found.
    assert_eq!(resolve_model_path(None, home, |_| false), None);
}

#[test]
fn audio_filters_run_before_whisper() {
    let called = std::cell::Cell::new(false);
    let t = |_: &[f32]| {
        called.set(true);
        Ok("hi".to_string())
    };
    // Too short.
    assert_eq!(process_audio(&[0.5; 100], t), Outcome::Nothing);
    assert!(!called.get());
    // Long enough but silent.
    let t = |_: &[f32]| {
        called.set(true);
        Ok("hi".to_string())
    };
    assert_eq!(process_audio(&vec![0.0; 16000], t), Outcome::Nothing);
    assert!(!called.get());
}

#[test]
fn audio_with_speech_level_is_transcribed_and_sanitized() {
    let loud = vec![0.2f32; 16000];
    assert_eq!(
        process_audio(&loud, |_| Ok("hello there".to_string())),
        Outcome::Text("hello there".into())
    );
    assert_eq!(
        process_audio(&loud, |_| Ok("".to_string())),
        Outcome::Nothing
    );
    assert_eq!(
        process_audio(&loud, |_| Err("Model missing".to_string())),
        Outcome::Failed("Model missing".into())
    );
}

#[test]
fn paste_errors_map_to_visible_status() {
    assert_eq!(
        paste_error_status(&PasteError::AccessibilityDenied),
        "Copied: paste blocked (Accessibility)"
    );
    assert_eq!(
        paste_error_status(&PasteError::ClipboardError("x".into())),
        "Copied: paste failed"
    );
    assert_eq!(
        paste_error_status(&PasteError::InjectFailed("x".into())),
        "Copied: paste failed"
    );
}

#[test]
fn mic_gate_maps_permission() {
    assert_eq!(mic_gate(MicPermissionStatus::Granted), MicGate::Go);
    assert_eq!(mic_gate(MicPermissionStatus::Undetermined), MicGate::Ask);
    assert_eq!(mic_gate(MicPermissionStatus::Denied), MicGate::Denied);
}

#[test]
fn drive_publishes_status_and_runs_effects() {
    let (tx, rx) = channel();
    tx.send(hk(HotkeyAction::StartRecording)).unwrap();
    tx.send(hk(HotkeyAction::StopRecording)).unwrap();
    tx.send(Input::Transcribed(Outcome::Text("t".into())))
        .unwrap();
    tx.send(Input::Pasted(Ok(()))).unwrap();
    drop(tx);

    let mut statuses = Vec::new();
    let mut effects = Vec::new();
    drive(rx, |s| statuses.push(s.to_string()), |e| effects.push(e));
    assert_eq!(statuses, vec!["REC", "...", "...", ""]);
    assert_eq!(
        effects,
        vec![
            Effect::StartCapture,
            Effect::StopAndTranscribe,
            Effect::Paste("t".into())
        ]
    );
}
