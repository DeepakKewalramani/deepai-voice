//! Pure dictation state machine. No I/O: inputs in, at most one effect out.

use std::collections::VecDeque;

use super::Outcome;
use super::stream_text::SegmentTyping;
use crate::focus::StopReason;
use crate::hotkey::HotkeyAction;

/// Where one dictation currently is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictationState {
    Idle,
    Recording,
    Transcribing,
    Pasting,
    /// Visible error text; the next hotkey press starts over.
    Error(String),
    /// The user pressed Esc. Shows "Cancelled" briefly; the next fresh
    /// press starts a new hold, the release of the cancelled one is ignored.
    Cancelled,
}

/// Everything the machine reacts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Hotkey(HotkeyAction),
    /// Mic permission or capture start failed.
    StartFailed(String),
    /// Final text of the release tail (the last segment of the hold).
    Transcribed(Outcome),
    /// Final text of a segment that ended at a pause while the key was held.
    Segment {
        seq: u64,
        outcome: Outcome,
    },
    /// Sanitized partial hypothesis of the open segment `seq`.
    Partial {
        seq: u64,
        text: String,
    },
    /// Partial of the streaming engine for segment `seq` (one engine session).
    /// Typed eagerly: every word but the newest last one.
    StreamPartial {
        seq: u64,
        text: String,
    },
    /// Result of the last Paste or Correct; the error is the status text.
    Pasted(Result<(), String>),
    /// A focus or secure-field guard stopped the last Paste or Correct.
    Stopped(StopReason),
    /// `input` produced by worker threads for hold `hold`. Dropped unless it
    /// is the current hold, so a late result of an earlier hold (segment seq
    /// restarts at 1 every hold) never types into the next one.
    FromHold {
        hold: u64,
        input: Box<Input>,
    },
    /// Esc while a hold is active: drop everything, type nothing more.
    Cancel,
    ClearError,
}

/// Work the caller must do after a transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    StartCapture,
    StopAndTranscribe,
    DiscardCapture,
    /// Type text at the cursor with key events. Never via the clipboard.
    Paste(String),
    /// Press Backspace `backspaces` times, then type `text`.
    Correct {
        backspaces: usize,
        text: String,
    },
    /// An error is showing; clear it after a delay.
    ScheduleClearError,
    /// Esc: stop the recorder, drop queued audio and Whisper jobs, stop the
    /// typing thread for this hold (no Backspace), clear "Cancelled" after a
    /// short delay with `Input::ClearError`.
    CancelHold,
}

/// Per-hold bookkeeping for live typing.
#[derive(Debug, Default)]
struct Hold {
    /// Highest segment whose final text has arrived.
    last_final_seq: u64,
    /// Typing state of the open segment.
    open: Option<(u64, SegmentTyping)>,
    /// Some earlier segment of this hold typed text, so the next starts with
    /// a space. Always false at hold start: a new hold never adds a leading
    /// space, because the caret may have moved and we cannot read it.
    any_text: bool,
    queue: VecDeque<Effect>,
    /// An edit is being typed right now.
    in_flight: bool,
    /// Typing stopped for the rest of the hold; this is the status text.
    paste_error: Option<String>,
    /// A secure field was detected: also hide the live preview.
    secure: bool,
    error: Option<String>,
    /// The release tail has been transcribed.
    tail_done: bool,
}

#[derive(Debug)]
pub struct Machine {
    state: DictationState,
    /// Id of the current (or last) hold. Rises by one per StartCapture.
    hold_id: u64,
    hold: Hold,
    partial: String,
    followups: VecDeque<Effect>,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    pub fn new() -> Self {
        Self {
            state: DictationState::Idle,
            hold_id: 0,
            hold: Hold::default(),
            partial: String::new(),
            followups: VecDeque::new(),
        }
    }

    /// Id of the current hold. Every `Effect::StartCapture` starts hold
    /// `hold_id()`; workers tag their results with it (`Input::FromHold`).
    pub fn hold_id(&self) -> u64 {
        self.hold_id
    }

    pub fn state(&self) -> &DictationState {
        &self.state
    }

    /// Live preview: the open segment's words not typed yet. Empty when none.
    pub fn partial(&self) -> &str {
        &self.partial
    }

    /// Further effects from the last `handle` call, in order.
    pub fn take_followup(&mut self) -> Option<Effect> {
        self.followups.pop_front()
    }

    /// Text for the tray and window title. Empty means idle.
    pub fn status_text(&self) -> String {
        match &self.state {
            DictationState::Idle => String::new(),
            DictationState::Recording => "REC".into(),
            DictationState::Transcribing | DictationState::Pasting => "...".into(),
            DictationState::Error(msg) => msg.clone(),
            DictationState::Cancelled => "Cancelled".into(),
        }
    }

    pub fn handle(&mut self, input: Input) -> Option<Effect> {
        let mut out = Vec::new();
        self.step(input, &mut out);
        if self.state != DictationState::Recording {
            self.partial.clear();
        }
        let mut it = out.into_iter();
        let first = it.next();
        self.followups.extend(it);
        first
    }

    fn step(&mut self, input: Input, out: &mut Vec<Effect>) {
        use DictationState as S;
        match (&self.state, input) {
            (_, Input::FromHold { hold, input }) => {
                if hold == self.hold_id {
                    self.step(*input, out);
                }
            }
            (S::Idle | S::Error(_) | S::Cancelled, Input::Hotkey(HotkeyAction::StartRecording)) => {
                self.state = S::Recording;
                self.hold_id += 1;
                self.hold = Hold::default();
                self.followups.clear();
                out.push(Effect::StartCapture);
            }
            (S::Recording, Input::Hotkey(HotkeyAction::StopRecording)) => {
                self.state = S::Transcribing;
                out.push(Effect::StopAndTranscribe);
            }
            (S::Recording, Input::Hotkey(HotkeyAction::DiscardRecording)) => {
                self.state = S::Idle;
                out.push(Effect::DiscardCapture);
            }
            (S::Idle | S::Recording | S::Transcribing, Input::StartFailed(msg)) => {
                self.state = S::Error(msg);
                out.push(Effect::ScheduleClearError);
            }
            (S::Recording, Input::Partial { seq, text }) => self.on_partial(seq, &text, false, out),
            (S::Recording, Input::StreamPartial { seq, text }) => {
                self.on_partial(seq, &text, true, out)
            }
            (S::Recording | S::Transcribing, Input::Segment { seq, outcome }) => {
                self.on_final(Some(seq), outcome, out);
            }
            (S::Transcribing, Input::Transcribed(outcome)) => {
                self.hold.tail_done = true;
                self.on_final(None, outcome, out);
                self.state = S::Pasting;
                self.try_finish(out);
            }
            (S::Recording | S::Transcribing | S::Pasting, Input::Pasted(result)) => {
                self.on_pasted(result, out);
                self.try_finish(out);
            }
            (S::Recording | S::Transcribing | S::Pasting, Input::Stopped(reason)) => {
                if reason == StopReason::SecureField {
                    self.hold.secure = true;
                }
                self.on_pasted(Err(reason.status().to_string()), out);
                self.try_finish(out);
            }
            (S::Recording | S::Transcribing | S::Pasting, Input::Cancel) => {
                self.state = S::Cancelled;
                // Words already typed stay; nothing queued is typed.
                self.hold = Hold::default();
                out.push(Effect::CancelHold);
            }
            (S::Error(_) | S::Cancelled, Input::ClearError) => {
                self.state = S::Idle;
            }
            _ => {}
        }
    }

    fn on_partial(&mut self, seq: u64, text: &str, eager: bool, out: &mut Vec<Effect>) {
        let hold = &mut self.hold;
        if seq <= hold.last_final_seq {
            return;
        }
        if hold.open.as_ref().is_none_or(|(s, _)| *s != seq) {
            let typing = if eager {
                SegmentTyping::eager(hold.any_text)
            } else {
                SegmentTyping::new(hold.any_text)
            };
            hold.open = Some((seq, typing));
        }
        let Some((_, typing)) = hold.open.as_mut() else {
            return;
        };
        if hold.secure {
            self.partial.clear();
            return;
        }
        if hold.paste_error.is_some() {
            self.partial = text.split_whitespace().collect::<Vec<_>>().join(" ");
            return;
        }
        let add = typing.on_hypothesis(text);
        self.partial = typing.preview(text);
        if let Some(add) = add {
            self.enqueue(Effect::Paste(add), out);
        }
    }

    /// A segment's final text. `seq` is None for the release tail, which
    /// belongs to whatever segment is open.
    fn on_final(&mut self, seq: Option<u64>, outcome: Outcome, out: &mut Vec<Effect>) {
        self.partial.clear();
        let hold = &mut self.hold;
        let mut typing = match (hold.open.take(), seq) {
            (Some((s, t)), Some(seq)) if s == seq => t,
            (Some((_, t)), None) => t,
            _ => SegmentTyping::new(hold.any_text),
        };
        if let Some(seq) = seq {
            hold.last_final_seq = hold.last_final_seq.max(seq);
        }
        let final_text = match outcome {
            Outcome::Text(t) => Some(t),
            Outcome::Nothing => None,
            Outcome::Failed(msg) => {
                hold.error.get_or_insert(msg);
                // Keep what was typed; there is nothing better to replace it with.
                hold.any_text |= !typing.typed().is_empty();
                return;
            }
        };
        let edit = typing.on_final(final_text.as_deref());
        hold.any_text |= !typing.typed().is_empty();
        if hold.paste_error.is_some() {
            // Typing stopped for this hold: nothing more is typed or copied.
            return;
        }
        if edit.backspaces > 0 {
            self.enqueue(
                Effect::Correct {
                    backspaces: edit.backspaces,
                    text: edit.text,
                },
                out,
            );
        } else if !edit.text.is_empty() {
            self.enqueue(Effect::Paste(edit.text), out);
        }
    }

    fn enqueue(&mut self, effect: Effect, out: &mut Vec<Effect>) {
        if !self.hold.in_flight {
            self.hold.in_flight = true;
            out.push(effect);
        } else {
            self.hold.queue.push_back(effect);
        }
    }

    fn on_pasted(&mut self, result: Result<(), String>, out: &mut Vec<Effect>) {
        let hold = &mut self.hold;
        if !std::mem::take(&mut hold.in_flight) {
            return;
        }
        match result {
            Ok(()) => {
                if let Some(next) = hold.queue.pop_front() {
                    hold.in_flight = true;
                    out.push(next);
                }
            }
            Err(msg) => {
                // Stop typing for the rest of the hold. Nothing is copied.
                hold.paste_error.get_or_insert(msg);
                hold.queue.clear();
            }
        }
    }

    /// After release: once the tail is in and typing is done, go idle or
    /// show the first error of the hold.
    fn try_finish(&mut self, out: &mut Vec<Effect>) {
        let hold = &self.hold;
        if self.state != DictationState::Pasting
            || !hold.tail_done
            || hold.in_flight
            || !hold.queue.is_empty()
        {
            return;
        }
        match hold.paste_error.clone().or_else(|| hold.error.clone()) {
            Some(msg) => {
                self.state = DictationState::Error(msg);
                out.push(Effect::ScheduleClearError);
            }
            None => self.state = DictationState::Idle,
        }
    }
}
