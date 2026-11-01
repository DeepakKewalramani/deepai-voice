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
    m.handle(Input::Pasted(Err("Accessibility needed".into())));
    assert_eq!(*m.state(), S::Error("Accessibility needed".into()));
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
        "Accessibility needed"
    );
    assert_eq!(
        paste_error_status(&PasteError::InjectFailed("x".into())),
        "Typing failed"
    );
    // Fixed strings only: never echoes the error detail.
    assert!(!paste_error_status(&PasteError::InjectFailed("secret".into())).contains("secret"));
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

// ---------------------------------------------------------------------------
// Live dictation: segmenter, job queue and the segment-aware machine.
// ---------------------------------------------------------------------------

use deepai_voice_lib::dictation::drive_live;
use deepai_voice_lib::dictation::live::{Job, JobQueue, Segmenter, process_segment};

const RATE: usize = 16000;

fn ms(n: usize) -> usize {
    n * RATE / 1000
}

/// Speech-level tone.
fn voice(len_ms: usize) -> Vec<f32> {
    (0..ms(len_ms))
        .map(|i| (i as f32 * 0.07).sin() * 0.2)
        .collect()
}

fn quiet(len_ms: usize) -> Vec<f32> {
    vec![0.0; ms(len_ms)]
}

fn cat(parts: &[Vec<f32>]) -> Vec<f32> {
    parts.concat()
}

#[test]
fn segmenter_ignores_silence_and_short_blips() {
    let mut s = Segmenter::new();
    assert!(s.push(&quiet(5000)).is_empty());
    assert!(!s.is_open());
    assert!(s.push(&cat(&[voice(150), quiet(2000)])).is_empty());
    assert!(!s.is_open());
    assert!(s.flush().is_none());
}

#[test]
fn segmenter_ends_a_segment_after_a_pause() {
    let mut s = Segmenter::new();
    assert!(s.push(&voice(1000)).is_empty());
    assert!(s.is_open());
    let done = s.push(&quiet(700));
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].seq, 1);
    // About one second of speech plus short pre-roll and tail, never the full pause.
    let len = done[0].samples.len();
    assert!(len >= ms(1000) && len <= ms(1600), "segment {len} samples");
    assert!(!s.is_open());
    assert!(s.flush().is_none());
}

#[test]
fn segmenter_keeps_short_pauses_inside_one_segment() {
    let mut s = Segmenter::new();
    let audio = cat(&[voice(800), quiet(400), voice(800), quiet(700)]);
    let done = s.push(&audio);
    assert_eq!(done.len(), 1);
    assert!(done[0].samples.len() >= ms(2000));
}

#[test]
fn segmenter_splits_two_utterances_with_rising_seq() {
    let mut s = Segmenter::new();
    let audio = cat(&[voice(600), quiet(800), voice(600), quiet(800)]);
    let done = s.push(&audio);
    assert_eq!(done.iter().map(|d| d.seq).collect::<Vec<_>>(), vec![1, 2]);
}

#[test]
fn segmenter_force_cuts_long_speech_at_8_seconds() {
    let mut s = Segmenter::new();
    let done = s.push(&voice(9000));
    assert_eq!(done.len(), 1);
    assert!(done[0].samples.len() <= ms(8000));
    assert!(done[0].samples.len() >= ms(7500));
    // The rest reopens as the next segment and comes out on flush.
    let tail = s.flush().expect("open tail");
    assert_eq!(tail.seq, 2);
    assert!(tail.samples.len() >= ms(500));
}

#[test]
fn segmenter_result_does_not_depend_on_chunk_size() {
    let audio = cat(&[voice(700), quiet(900), voice(1200), quiet(300)]);
    let mut whole = Segmenter::new();
    let mut a = whole.push(&audio);
    a.extend(whole.flush());
    let mut chunked = Segmenter::new();
    let mut b = Vec::new();
    for c in audio.chunks(37) {
        b.extend(chunked.push(c));
    }
    b.extend(chunked.flush());
    assert_eq!(a, b);
    assert_eq!(a.len(), 2);
}

#[test]
fn segmenter_partial_window_is_the_open_segment() {
    let mut s = Segmenter::new();
    assert!(s.open_audio().is_none());
    s.push(&voice(5000));
    let (seq, audio) = s.open_audio().expect("open");
    assert_eq!(seq, 1);
    assert!(audio.len() >= ms(5000) && audio.len() <= ms(5500));
    s.push(&quiet(700));
    assert!(s.open_audio().is_none());
}

#[test]
fn job_queue_runs_finals_first_and_drops_stale_partials() {
    let mut q = JobQueue::default();
    assert!(q.is_idle());
    q.push_partial(1, vec![0.1]);
    q.push_partial(1, vec![0.2]); // replaces the older partial
    q.push_final(1, vec![0.3], false); // makes the seq 1 partial stale
    q.push_partial(2, vec![0.4]);
    assert!(!q.is_idle());
    assert_eq!(
        q.take_next(),
        Some(Job::Final {
            seq: 1,
            samples: vec![0.3],
            last: false
        })
    );
    assert!(!q.is_idle(), "a running job keeps the queue busy");
    q.done();
    assert_eq!(
        q.take_next(),
        Some(Job::Partial {
            seq: 2,
            samples: vec![0.4]
        })
    );
    q.done();
    assert_eq!(q.take_next(), None);
    assert!(q.is_idle());
    q.push_final(3, vec![], true);
    q.push_partial(4, vec![0.5]);
    q.clear();
    assert_eq!(q.take_next(), None);
}

#[test]
fn process_segment_sanitizes_and_pads_short_speech() {
    let short = voice(200);
    let got = process_segment(&short, |s| {
        assert!(s.len() >= 4800, "padded to the minimum length");
        Ok("hello there".to_string())
    });
    assert_eq!(got, Outcome::Text("hello there".into()));
    assert_eq!(
        process_segment(&short, |_| Ok("[BLANK_AUDIO]".into())),
        Outcome::Nothing
    );
    assert_eq!(
        process_segment(&short, |_| Err("Model missing".into())),
        Outcome::Failed("Model missing".into())
    );
    assert_eq!(process_segment(&[], |_| Ok("x".into())), Outcome::Nothing);
}

fn seg(seq: u64, text: &str) -> Input {
    Input::Segment {
        seq,
        outcome: Outcome::Text(text.into()),
    }
}

fn recording() -> Machine {
    let mut m = Machine::new();
    m.handle(hk(HotkeyAction::StartRecording));
    m
}

#[test]
fn live_segment_is_pasted_while_still_recording() {
    let mut m = recording();
    assert_eq!(
        m.handle(seg(1, "hello")),
        Some(Effect::Paste("hello".into()))
    );
    assert_eq!(*m.state(), S::Recording);
    assert_eq!(m.status_text(), "REC");
}

#[test]
fn later_segments_get_one_space_and_wait_for_the_previous_paste() {
    let mut m = recording();
    m.handle(seg(1, "hello"));
    assert_eq!(m.handle(seg(2, "world")), None);
    assert_eq!(
        m.handle(Input::Pasted(Ok(()))),
        Some(Effect::Paste(" world".into()))
    );
    assert_eq!(m.handle(Input::Pasted(Ok(()))), None);
    assert_eq!(*m.state(), S::Recording);
    // Empty segments do not count as pasted text.
    m.handle(Input::Segment {
        seq: 3,
        outcome: Outcome::Nothing,
    });
    assert_eq!(
        m.handle(seg(4, "again")),
        Some(Effect::Paste(" again".into()))
    );
}

#[test]
fn a_new_hold_starts_without_a_space() {
    let mut m = recording();
    m.handle(seg(1, "one"));
    m.handle(Input::Pasted(Ok(())));
    m.handle(hk(HotkeyAction::StopRecording));
    m.handle(Input::Transcribed(Outcome::Nothing));
    assert_eq!(*m.state(), S::Idle);
    m.handle(hk(HotkeyAction::StartRecording));
    assert_eq!(m.handle(seg(1, "two")), Some(Effect::Paste("two".into())));
}

#[test]
fn release_flushes_tail_after_earlier_pastes() {
    let mut m = recording();
    m.handle(seg(1, "first"));
    assert_eq!(
        m.handle(hk(HotkeyAction::StopRecording)),
        Some(Effect::StopAndTranscribe)
    );
    assert_eq!(
        m.handle(Input::Transcribed(Outcome::Text("last".into()))),
        None
    );
    assert_eq!(*m.state(), S::Pasting);
    assert_eq!(
        m.handle(Input::Pasted(Ok(()))),
        Some(Effect::Paste(" last".into()))
    );
    assert_eq!(m.handle(Input::Pasted(Ok(()))), None);
    assert_eq!(*m.state(), S::Idle);
}

#[test]
fn empty_tail_waits_for_pending_paste_then_idles() {
    let mut m = recording();
    m.handle(seg(1, "only"));
    m.handle(hk(HotkeyAction::StopRecording));
    assert_eq!(m.handle(Input::Transcribed(Outcome::Nothing)), None);
    assert_eq!(*m.state(), S::Pasting);
    m.handle(Input::Pasted(Ok(())));
    assert_eq!(*m.state(), S::Idle);
}

#[test]
fn late_segment_after_release_is_still_pasted_in_order() {
    let mut m = recording();
    m.handle(hk(HotkeyAction::StopRecording));
    assert_eq!(m.handle(seg(1, "late")), Some(Effect::Paste("late".into())));
    assert_eq!(*m.state(), S::Transcribing);
    m.handle(Input::Pasted(Ok(())));
    assert_eq!(*m.state(), S::Transcribing);
    m.handle(Input::Transcribed(Outcome::Text("tail".into())));
    assert_eq!(*m.state(), S::Pasting);
}

#[test]
fn typing_failure_stops_typing_for_the_hold_and_copies_nothing() {
    let mut m = recording();
    m.handle(seg(1, "a"));
    assert_eq!(m.handle(Input::Pasted(Err("Typing failed".into()))), None);
    assert_eq!(m.take_followup(), None);
    assert_eq!(*m.state(), S::Recording, "the hold keeps going");
    assert_eq!(m.handle(seg(2, "b")), None, "nothing typed after a failure");
    assert_eq!(m.take_followup(), None);
    m.handle(hk(HotkeyAction::StopRecording));
    assert_eq!(
        m.handle(Input::Transcribed(Outcome::Text("c".into()))),
        Some(Effect::ScheduleClearError)
    );
    assert_eq!(*m.state(), S::Error("Typing failed".into()));
    assert_eq!(m.take_followup(), None);
}

#[test]
fn segment_failure_shows_error_at_the_end_of_the_hold() {
    let mut m = recording();
    m.handle(Input::Segment {
        seq: 1,
        outcome: Outcome::Failed("Transcription failed".into()),
    });
    assert_eq!(*m.state(), S::Recording);
    m.handle(hk(HotkeyAction::StopRecording));
    assert_eq!(
        m.handle(Input::Transcribed(Outcome::Nothing)),
        Some(Effect::ScheduleClearError)
    );
    assert_eq!(*m.state(), S::Error("Transcription failed".into()));
}

#[test]
fn partial_text_shows_only_for_the_open_segment_while_recording() {
    let mut m = recording();
    assert_eq!(m.partial(), "");
    m.handle(Input::Partial {
        seq: 1,
        text: "hel".into(),
    });
    assert_eq!(m.partial(), "hel");
    // Commit clears it, and a late partial for the committed segment is dropped.
    m.handle(seg(1, "hello"));
    assert_eq!(m.partial(), "");
    m.handle(Input::Partial {
        seq: 1,
        text: "hello".into(),
    });
    assert_eq!(m.partial(), "");
    m.handle(Input::Partial {
        seq: 2,
        text: "wor".into(),
    });
    assert_eq!(m.partial(), "wor");
    // Release clears it and later partials are ignored.
    m.handle(hk(HotkeyAction::StopRecording));
    assert_eq!(m.partial(), "");
    m.handle(Input::Partial {
        seq: 3,
        text: "x".into(),
    });
    assert_eq!(m.partial(), "");
}

#[test]
fn drive_live_publishes_partials_and_followups() {
    let (tx, rx) = channel();
    tx.send(hk(HotkeyAction::StartRecording)).unwrap();
    tx.send(Input::Partial {
        seq: 1,
        text: "hi".into(),
    })
    .unwrap();
    tx.send(seg(1, "hi")).unwrap();
    tx.send(Input::Pasted(Err("Typing failed".into()))).unwrap();
    tx.send(hk(HotkeyAction::StopRecording)).unwrap();
    tx.send(Input::Transcribed(Outcome::Nothing)).unwrap();
    drop(tx);

    let mut partials = Vec::new();
    let mut effects = Vec::new();
    drive_live(
        rx,
        |_| {},
        |p| partials.push(p.to_string()),
        |e| effects.push(e),
    );
    assert_eq!(partials, vec!["hi", ""]);
    assert_eq!(
        effects,
        vec![
            Effect::StartCapture,
            Effect::Paste("hi".into()),
            Effect::StopAndTranscribe,
            Effect::ScheduleClearError,
        ]
    );
}

fn partial(seq: u64, text: &str) -> Input {
    Input::Partial {
        seq,
        text: text.into(),
    }
}

#[test]
fn agreed_partial_words_are_typed_live_and_fixed_by_the_final() {
    let mut m = recording();
    assert_eq!(m.handle(partial(1, "hello world")), None);
    assert_eq!(m.partial(), "hello world");
    assert_eq!(
        m.handle(partial(1, "hello world how")),
        Some(Effect::Paste("hello world".into()))
    );
    // Only the not yet typed tail is previewed.
    assert_eq!(m.partial(), "how");
    // The next agreed word waits for the typing in flight.
    assert_eq!(m.handle(partial(1, "hello world how are")), None);
    assert_eq!(
        m.handle(Input::Pasted(Ok(()))),
        Some(Effect::Paste(" how".into()))
    );
    m.handle(Input::Pasted(Ok(())));
    assert_eq!(
        m.handle(seg(1, "Hello world, how are you.")),
        Some(Effect::Correct {
            backspaces: 15,
            text: "Hello world, how are you.".into()
        })
    );
    assert_eq!(m.partial(), "");
    m.handle(Input::Pasted(Ok(())));
    // Next segment: a single leading space before its first typed words.
    m.handle(partial(2, "next one"));
    assert_eq!(
        m.handle(partial(2, "next one two")),
        Some(Effect::Paste(" next one".into()))
    );
    m.handle(Input::Pasted(Ok(())));
    m.handle(hk(HotkeyAction::StopRecording));
    assert_eq!(
        m.handle(Input::Transcribed(Outcome::Text("next one two".into()))),
        Some(Effect::Paste(" two".into()))
    );
    m.handle(Input::Pasted(Ok(())));
    assert_eq!(*m.state(), S::Idle);
}

#[test]
fn final_that_filters_to_nothing_removes_live_typed_words() {
    let mut m = recording();
    m.handle(partial(1, "thank you so"));
    m.handle(partial(1, "thank you so much"));
    m.handle(Input::Pasted(Ok(())));
    m.handle(hk(HotkeyAction::StopRecording));
    assert_eq!(
        m.handle(Input::Transcribed(Outcome::Nothing)),
        Some(Effect::Correct {
            backspaces: 12,
            text: String::new()
        })
    );
    m.handle(Input::Pasted(Ok(())));
    assert_eq!(*m.state(), S::Idle);
}

#[test]
fn partials_do_not_type_after_a_failure() {
    let mut m = recording();
    m.handle(seg(1, "a"));
    m.handle(Input::Pasted(Err("Typing failed".into())));
    m.handle(partial(2, "b c"));
    assert_eq!(m.handle(partial(2, "b c d")), None);
    assert_eq!(m.partial(), "b c d");
}

// ---------------------------------------------------------------------------
// Hold ids, typing guards and the partial paste fallback.
// ---------------------------------------------------------------------------

use deepai_voice_lib::dictation::run_edit_with;
use deepai_voice_lib::focus::{Gate, StopReason, TypingGuard};
use deepai_voice_lib::paste::typing::{EditError, EditOutcome};

fn tagged(hold: u64, input: Input) -> Input {
    Input::FromHold {
        hold,
        input: Box::new(input),
    }
}

#[test]
fn every_started_hold_gets_a_new_id() {
    let mut m = Machine::new();
    assert_eq!(m.hold_id(), 0);
    m.handle(hk(HotkeyAction::StartRecording));
    assert_eq!(m.hold_id(), 1);
    m.handle(hk(HotkeyAction::StartRecording)); // ignored while recording
    assert_eq!(m.hold_id(), 1);
    m.handle(hk(HotkeyAction::DiscardRecording));
    m.handle(hk(HotkeyAction::StartRecording));
    assert_eq!(m.hold_id(), 2);
}

#[test]
fn late_results_from_a_previous_hold_are_dropped() {
    let mut m = recording();
    m.handle(tagged(1, seg(1, "one")));
    m.handle(hk(HotkeyAction::DiscardRecording));
    m.handle(hk(HotkeyAction::StartRecording));
    assert_eq!(m.hold_id(), 2);
    // seq restarts at 1, so only the hold id can tell these apart.
    assert_eq!(m.handle(tagged(1, seg(1, "old"))), None);
    assert_eq!(m.handle(tagged(1, partial(1, "old words here"))), None);
    assert_eq!(m.partial(), "");
    assert_eq!(
        m.handle(tagged(2, seg(1, "new"))),
        Some(Effect::Paste("new".into()))
    );
    // A typing result from hold 1 must not release hold 2's edit in flight.
    m.handle(tagged(2, seg(2, "next")));
    assert_eq!(m.handle(tagged(1, Input::Pasted(Ok(())))), None);
    assert_eq!(
        m.handle(tagged(2, Input::Pasted(Ok(())))),
        Some(Effect::Paste(" next".into()))
    );
    // A stale release tail does not end the current hold.
    m.handle(tagged(1, Input::Transcribed(Outcome::Text("x".into()))));
    assert_eq!(*m.state(), S::Recording);
}

#[test]
fn app_change_stops_typing_for_the_rest_of_the_hold() {
    let mut m = recording();
    m.handle(seg(1, "a"));
    assert_eq!(m.handle(Input::Stopped(StopReason::AppChanged)), None);
    assert_eq!(m.take_followup(), None);
    assert_eq!(*m.state(), S::Recording, "the hold keeps going");
    assert_eq!(m.handle(seg(2, "b")), None);
    m.handle(hk(HotkeyAction::StopRecording));
    m.handle(Input::Transcribed(Outcome::Nothing));
    assert_eq!(*m.state(), S::Error("Typing paused: app changed".into()));
}

#[test]
fn secure_field_types_nothing_and_never_touches_the_clipboard() {
    let mut m = recording();
    m.handle(seg(1, "secret"));
    assert_eq!(m.handle(Input::Stopped(StopReason::SecureField)), None);
    assert_eq!(m.take_followup(), None);
    assert_eq!(m.handle(seg(2, "more")), None);
    m.handle(partial(3, "still talking"));
    assert_eq!(
        m.partial(),
        "",
        "no preview of text meant for a secure field"
    );
    m.handle(hk(HotkeyAction::StopRecording));
    assert_eq!(
        m.handle(Input::Transcribed(Outcome::Text("tail".into()))),
        Some(Effect::ScheduleClearError)
    );
    assert_eq!(m.take_followup(), None);
    assert_eq!(*m.state(), S::Error("Secure field: not typing".into()));
}

#[derive(Default)]
struct FixedGuard(Option<Gate>);

impl TypingGuard for FixedGuard {
    fn check(&mut self) -> Gate {
        self.0.clone().unwrap_or(Gate::Go)
    }
}

#[test]
fn failed_typing_reports_a_fixed_status_and_never_pastes() {
    let input = run_edit_with(&mut FixedGuard::default(), 0, "hello world", |_, _, _| {
        Err(EditError {
            error: PasteError::InjectFailed("x".into()),
            typed_bytes: Some(6),
        })
    });
    assert_eq!(input, Input::Pasted(Err("Typing failed".into())));
    let input = run_edit_with(&mut FixedGuard::default(), 2, "hi", |_, _, _| {
        Err(EditError {
            error: PasteError::AccessibilityDenied,
            typed_bytes: None,
        })
    });
    assert_eq!(input, Input::Pasted(Err("Accessibility needed".into())));
}

#[test]
fn guard_stops_map_to_stopped_and_skipped_corrections_count_as_done() {
    let input = run_edit_with(&mut FixedGuard::default(), 0, "x", |_, _, _| {
        Ok(EditOutcome::Stopped(StopReason::AppChanged))
    });
    assert_eq!(input, Input::Stopped(StopReason::AppChanged));
    let input = run_edit_with(&mut FixedGuard::default(), 0, "pw", |_, _, _| {
        Ok(EditOutcome::Stopped(StopReason::SecureField))
    });
    assert_eq!(input, Input::Stopped(StopReason::SecureField));
    let input = run_edit_with(&mut FixedGuard::default(), 3, "x", |_, _, _| {
        Ok(EditOutcome::SkippedCorrection)
    });
    assert_eq!(input, Input::Pasted(Ok(())));
    // Empty edit: the typer is never called.
    let input = run_edit_with(&mut FixedGuard::default(), 0, "", |_, _, _| {
        panic!("must not type")
    });
    assert_eq!(input, Input::Pasted(Ok(())));
}

#[test]
fn job_queue_begin_hold_drops_earlier_work_and_tags_new_jobs() {
    let mut q = JobQueue::default();
    q.begin_hold(1);
    q.push_final(1, vec![0.1], false);
    q.push_partial(2, vec![0.2]);
    q.begin_hold(2);
    assert_eq!(q.hold(), 2);
    assert_eq!(q.take_next(), None, "hold 1 work is gone");
    q.push_final(1, vec![0.3], false);
    assert!(q.take_next().is_some());
    assert_eq!(q.hold(), 2);
}

// ---------------------------------------------------------------------------
// Esc cancels the hold.
// ---------------------------------------------------------------------------

use deepai_voice_lib::dictation::escape_wanted;

#[test]
fn cancel_while_recording_stops_everything_and_shows_cancelled() {
    let mut m = recording();
    m.handle(partial(1, "hello world"));
    assert_eq!(
        m.handle(seg(1, "hello")),
        Some(Effect::Paste("hello".into()))
    );
    m.handle(seg(2, "queued"));
    assert_eq!(m.handle(Input::Cancel), Some(Effect::CancelHold));
    assert_eq!(m.take_followup(), None);
    assert_eq!(*m.state(), S::Cancelled);
    assert_eq!(m.status_text(), "Cancelled");
    assert_eq!(m.partial(), "");
    // The typing result of the edit in flight releases nothing queued.
    assert_eq!(m.handle(Input::Pasted(Ok(()))), None);
    // Late results of the cancelled hold do nothing.
    assert_eq!(m.handle(seg(3, "late")), None);
    assert_eq!(m.handle(partial(4, "late words")), None);
    assert_eq!(
        m.handle(Input::Transcribed(Outcome::Text("x".into()))),
        None
    );
    assert_eq!(*m.state(), S::Cancelled);
    // The status clears back to idle.
    m.handle(Input::ClearError);
    assert_eq!(*m.state(), S::Idle);
}

#[test]
fn cancel_while_thinking_or_typing_the_last_words() {
    let mut m = recording();
    m.handle(hk(HotkeyAction::StopRecording));
    assert_eq!(*m.state(), S::Transcribing);
    assert_eq!(m.handle(Input::Cancel), Some(Effect::CancelHold));
    assert_eq!(
        m.handle(Input::Transcribed(Outcome::Text("x".into()))),
        None
    );
    assert_eq!(*m.state(), S::Cancelled);

    let mut m = recording();
    m.handle(seg(1, "a"));
    m.handle(hk(HotkeyAction::StopRecording));
    m.handle(Input::Transcribed(Outcome::Text("b".into())));
    assert_eq!(*m.state(), S::Pasting);
    assert_eq!(m.handle(Input::Cancel), Some(Effect::CancelHold));
    assert_eq!(
        m.handle(Input::Pasted(Ok(()))),
        None,
        "no more typing, no Backspace"
    );
    assert_eq!(*m.state(), S::Cancelled);
}

#[test]
fn release_after_cancel_is_ignored_and_a_fresh_press_starts_a_new_hold() {
    let mut m = recording();
    m.handle(Input::Cancel);
    assert_eq!(m.handle(hk(HotkeyAction::StopRecording)), None);
    assert_eq!(*m.state(), S::Cancelled);
    assert_eq!(
        m.handle(hk(HotkeyAction::StartRecording)),
        Some(Effect::StartCapture)
    );
    assert_eq!(m.hold_id(), 2);
    // A late segment of the cancelled hold 1 is dropped in hold 2.
    assert_eq!(m.handle(tagged(1, seg(1, "old"))), None);
    // The old status timer does not end the new hold.
    m.handle(Input::ClearError);
    assert_eq!(*m.state(), S::Recording);
}

#[test]
fn cancel_when_idle_or_showing_an_error_is_a_no_op() {
    let mut m = Machine::new();
    assert_eq!(m.handle(Input::Cancel), None);
    assert_eq!(*m.state(), S::Idle);
    m.handle(hk(HotkeyAction::StartRecording));
    m.handle(Input::StartFailed("Mic denied in Settings".into()));
    m.take_followup();
    assert_eq!(m.handle(Input::Cancel), None);
    assert_eq!(*m.state(), S::Error("Mic denied in Settings".into()));
}

#[test]
fn escape_is_registered_only_while_a_hold_is_active() {
    assert!(!escape_wanted(&S::Idle));
    assert!(escape_wanted(&S::Recording));
    assert!(escape_wanted(&S::Transcribing));
    assert!(escape_wanted(&S::Pasting));
    assert!(!escape_wanted(&S::Cancelled));
    assert!(!escape_wanted(&S::Error("x".into())));
}

#[test]
fn escape_shortcut_parses_and_is_kept_apart_from_the_talk_key() {
    use deepai_voice_lib::hotkey::fallback::{is_escape, parse};
    let esc = parse(deepai_voice_lib::dictation::ESCAPE_SHORTCUT).expect("Escape parses");
    assert!(is_escape(&esc));
    assert!(!is_escape(&parse("Ctrl+Option+Space").unwrap()));
    assert!(!is_escape(&parse("Shift+Escape").unwrap()));
}

// ---------- 2.12 reliability: adaptive segmenter, safety net, partial cost ----------

use deepai_voice_lib::dictation::live::{
    NoiseFloor, PARTIAL_MIN_GAP, partial_due, safety_net_wanted,
};
use std::time::Duration;

/// Deterministic white noise with peak `amp`.
fn noise(len_ms: usize, amp: f32, seed: u32) -> Vec<f32> {
    let mut x = seed.max(1);
    (0..ms(len_ms))
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            ((x as f32 / u32::MAX as f32) * 2.0 - 1.0) * amp
        })
        .collect()
}

fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
    a.iter().zip(b).map(|(x, y)| x + y).collect()
}

/// Quiet speech: a tone with RMS about 0.004, under the old fixed 0.01 gate.
fn quiet_voice(len_ms: usize) -> Vec<f32> {
    (0..ms(len_ms))
        .map(|i| (i as f32 * 0.07).sin() * 0.006)
        .collect()
}

#[test]
fn segmenter_hears_quiet_speech_over_a_quiet_room() {
    let mut s = Segmenter::new();
    assert!(s.push(&noise(1000, 0.001, 7)).is_empty());
    assert!(!s.is_open());
    let speech = mix(&quiet_voice(1500), &noise(1500, 0.001, 9));
    s.push(&speech);
    assert!(s.is_open(), "quiet speech must open a segment");
    let done = s.push(&noise(800, 0.001, 11));
    assert_eq!(done.len(), 1);
}

#[test]
fn segmenter_learns_loud_steady_noise_and_stops_opening() {
    let mut s = Segmenter::new();
    // A fresh segmenter adapts within seconds; after that the noise is not voice.
    s.push(&noise(15_000, 0.08, 3));
    s.flush();
    let mut done = s.push(&noise(5_000, 0.08, 5));
    done.extend(s.flush());
    assert!(
        done.is_empty(),
        "steady noise opened {} segments",
        done.len()
    );
    // A segmenter that carries the learned floor rejects the noise at once.
    let mut next = Segmenter::with_floor(s.floor());
    let mut d = next.push(&noise(3_000, 0.08, 13));
    d.extend(next.flush());
    assert!(d.is_empty());
    // Speech well above the noise still opens.
    let loud = mix(&voice(1000), &noise(1000, 0.08, 17));
    next.push(&loud);
    assert!(next.is_open());
}

#[test]
fn noise_floor_has_a_sane_minimum_and_rises_slowly() {
    let mut f = NoiseFloor::default();
    for _ in 0..50 {
        assert!(!f.is_voice(0.0));
    }
    // Digital silence never makes a tiny click count as speech.
    assert!(!f.is_voice(0.001));
    assert!(f.is_voice(0.01));
    // One second of loud frames does not teach the floor that level yet.
    for _ in 0..50 {
        f.is_voice(0.1);
    }
    assert!(f.is_voice(0.1));
}

#[test]
fn safety_net_runs_only_for_a_real_hold_without_segments() {
    let speech = mix(&quiet_voice(600), &noise(600, 0.0005, 2));
    assert!(safety_net_wanted(0, &speech));
    // Segments were found: the normal path handles it.
    assert!(!safety_net_wanted(1, &speech));
    // Shorter than 0.4 s.
    assert!(!safety_net_wanted(0, &quiet_voice(300)));
    // Pure digital silence.
    assert!(!safety_net_wanted(0, &quiet(2000)));
}

#[test]
fn partials_wait_for_the_last_partial_cost_with_a_floor() {
    assert_eq!(PARTIAL_MIN_GAP, Duration::from_millis(300));
    let ms = Duration::from_millis;
    // Cheap partials: every 300 ms.
    assert!(!partial_due(ms(299), ms(100)));
    assert!(partial_due(ms(300), ms(100)));
    // Slow partials space themselves out by their own cost.
    assert!(!partial_due(ms(900), ms(1500)));
    assert!(partial_due(ms(1500), ms(1500)));
}
