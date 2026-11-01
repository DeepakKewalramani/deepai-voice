//! One held key on the streaming engine. Feeds drained audio into engine
//! sessions and turns their events into machine inputs. No I/O of its own and
//! no logging of text: tests drive it with `FakeStreamingStt`.
//!
//! Each engine session is one segment (`seq`). A session is restarted at a
//! natural pause (silence of `RESTART_PAUSE_SAMPLES` after it produced text)
//! or after `RESTART_MAX_SAMPLES` of audio, because engine requests are time
//! limited. Old sessions finish in the background while the new one already
//! gets audio; newer events are read only after every older final, so
//! segment finals reach the machine in order.
//!
//! The hold's dictionary (`HoldVocab`) is fixed at start: its words go to
//! every session as hints, and its correction map rewrites every partial and
//! final before the machine sees it (so typing works on corrected text only).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::TryRecvError;

use super::live::{NoiseFloor, SAMPLE_RATE, frame_rms};
use super::state::Input;
use super::{Outcome, apply_dictionary};
use crate::dictionary::HoldVocab;
use crate::stt::filters::sanitize_transcript;
use crate::stt::streaming::{StartOptions, StreamEvent, StreamingSession, StreamingStt};
use crate::stt::trait_def::ProviderError;

/// Silence after recognized text that ends a session (600 ms).
pub const RESTART_PAUSE_SAMPLES: usize = SAMPLE_RATE * 6 / 10;
/// Audio after which a session is restarted even without a pause (50 s).
pub const RESTART_MAX_SAMPLES: usize = SAMPLE_RATE * 50;
/// Fixed status when the engine cannot start a follow-up session mid-hold.
pub const STREAM_FAILED: &str = "Speech engine failed";
const FRAME: usize = SAMPLE_RATE / 50; // 20 ms

/// Text of a final (or of the last partial when a session ended without one).
fn outcome_of(text: &str) -> Outcome {
    match sanitize_transcript(text) {
        Some(t) => Outcome::Text(t),
        None => Outcome::Nothing,
    }
}

struct Open {
    seq: u64,
    session: Box<dyn StreamingSession>,
    /// Newest partial, used when the session ends with an error (for example
    /// "no speech detected") instead of a final.
    last_partial: String,
    /// Set when the session could not finish: this is its result.
    ended: Option<Outcome>,
}

impl Open {
    fn new(seq: u64, session: Box<dyn StreamingSession>) -> Self {
        Self {
            seq,
            session,
            last_partial: String::new(),
            ended: None,
        }
    }

    /// Stops the session; its result is the last partial.
    fn abandon(&mut self) {
        self.session.cancel();
        self.ended = Some(outcome_of(&self.last_partial));
    }

    /// Asks for the final; abandons the session when it cannot finish.
    fn finish_or_abandon(&mut self) {
        if self.session.finish().is_err() {
            self.abandon();
        }
    }
}

/// What one session's queued events amount to.
enum Drained {
    /// Still running; newest partial if any arrived.
    Running(Option<String>),
    /// Ended with this segment outcome.
    Ended(Outcome),
}

fn drain(open: &mut Open) -> Drained {
    if let Some(outcome) = open.ended.take() {
        return Drained::Ended(outcome);
    }
    let mut newest = None;
    loop {
        match open.session.events().try_recv() {
            Ok(StreamEvent::Partial(t)) => {
                open.last_partial = t.clone();
                newest = Some(t);
            }
            Ok(StreamEvent::Final(t)) => return Drained::Ended(outcome_of(&t)),
            // Error or closed without a final: keep what was recognized.
            Ok(StreamEvent::Error(_)) | Err(TryRecvError::Disconnected) => {
                return Drained::Ended(outcome_of(&open.last_partial));
            }
            Err(TryRecvError::Empty) => return Drained::Running(newest),
        }
    }
}

/// Streaming state of one hold.
pub struct StreamHold {
    engine: Arc<dyn StreamingStt>,
    locale: String,
    vocab: Arc<HoldVocab>,
    seq: u64,
    current: Option<Open>,
    /// Older sessions waiting for their final, oldest first.
    closing: VecDeque<Open>,
    session_samples: usize,
    silent_samples: usize,
    frame: Vec<f32>,
    /// Adaptive voice gate for pause detection; kept across sessions.
    gate: NoiseFloor,
    released: bool,
    failed: bool,
    done: bool,
}

impl StreamHold {
    /// Opens the first session. An error means: use Whisper for this hold.
    pub fn start(engine: Arc<dyn StreamingStt>, locale: &str) -> Result<Self, ProviderError> {
        Self::start_with_vocab(engine, locale, Arc::default())
    }

    /// Like `start`, with the hold's dictionary: hints for every session and
    /// corrections for every output.
    pub fn start_with_vocab(
        engine: Arc<dyn StreamingStt>,
        locale: &str,
        vocab: Arc<HoldVocab>,
    ) -> Result<Self, ProviderError> {
        let session = engine.start_with(locale, &Self::options(&vocab))?;
        Ok(Self {
            engine,
            locale: locale.to_string(),
            vocab,
            seq: 1,
            current: Some(Open::new(1, session)),
            closing: VecDeque::new(),
            session_samples: 0,
            silent_samples: 0,
            frame: Vec::with_capacity(FRAME),
            gate: NoiseFloor::default(),
            released: false,
            failed: false,
            done: false,
        })
    }

    fn options(vocab: &HoldVocab) -> StartOptions {
        StartOptions {
            hints: vocab.hints.clone(),
        }
    }

    fn corrected(&self, inputs: Vec<Input>) -> Vec<Input> {
        inputs
            .into_iter()
            .map(|i| apply_dictionary(i, &self.vocab))
            .collect()
    }

    /// Segment number of the newest session.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// True once the release tail was reported (or the hold was cancelled).
    pub fn is_done(&self) -> bool {
        self.done
    }

    fn track_silence(&mut self, samples: &[f32]) {
        for &s in samples {
            self.frame.push(s);
            if self.frame.len() == FRAME {
                let voice = self.gate.is_voice(frame_rms(&self.frame));
                self.silent_samples = if voice {
                    0
                } else {
                    self.silent_samples + FRAME
                };
                self.frame.clear();
            }
        }
    }

    fn open_next(&mut self) {
        self.seq += 1;
        self.session_samples = 0;
        self.silent_samples = 0;
        self.frame.clear();
        match self
            .engine
            .start_with(&self.locale, &Self::options(&self.vocab))
        {
            Ok(session) => self.current = Some(Open::new(self.seq, session)),
            Err(_) => self.failed = true,
        }
    }

    /// Audio drained while the key is held (may be empty). Returns the inputs
    /// for the machine, in order, with dictionary corrections applied.
    pub fn feed(&mut self, samples: &[f32]) -> Vec<Input> {
        let raw = self.feed_raw(samples);
        self.corrected(raw)
    }

    fn feed_raw(&mut self, samples: &[f32]) -> Vec<Input> {
        if self.released || self.done {
            return self.poll_raw();
        }
        if self.current.is_none() && !self.failed && !samples.is_empty() {
            self.open_next();
        }
        if let Some(mut open) = self.current.take() {
            if open.session.push_audio(samples).is_err() {
                open.abandon();
                self.closing.push_back(open);
            } else {
                self.session_samples += samples.len();
                self.track_silence(samples);
                let pause = !open.last_partial.trim().is_empty()
                    && self.silent_samples >= RESTART_PAUSE_SAMPLES;
                if pause || self.session_samples >= RESTART_MAX_SAMPLES {
                    // Partials already queued still belong to this segment.
                    let mut out = self.poll_open(&mut open);
                    open.finish_or_abandon();
                    self.closing.push_back(open);
                    self.open_next();
                    out.extend(self.poll_raw());
                    return out;
                }
                self.current = Some(open);
            }
        }
        self.poll_raw()
    }

    /// Newest partial of `open` as an input, only when no older session is
    /// still closing (so it cannot overtake an older final).
    fn poll_open(&mut self, open: &mut Open) -> Vec<Input> {
        let mut out = Vec::new();
        if !self.closing.is_empty() {
            return out;
        }
        match drain(open) {
            Drained::Running(Some(text)) => {
                if let Some(text) = sanitize_transcript(&text) {
                    out.push(Input::StreamPartial {
                        seq: open.seq,
                        text,
                    });
                }
            }
            // The session already sent its final: keep it for the restart,
            // so the segment result is the final and not the last partial.
            Drained::Ended(outcome) => open.ended = Some(outcome),
            Drained::Running(None) => {}
        }
        out
    }

    /// Reads engine events without new audio (dictionary corrections applied).
    pub fn poll(&mut self) -> Vec<Input> {
        let raw = self.poll_raw();
        self.corrected(raw)
    }

    fn poll_raw(&mut self) -> Vec<Input> {
        let mut out = Vec::new();
        if self.done {
            return out;
        }
        while let Some(front) = self.closing.front_mut() {
            match drain(front) {
                // Wait for its final before reading newer sessions.
                Drained::Running(_) => return out,
                Drained::Ended(outcome) => {
                    let seq = front.seq;
                    self.closing.pop_front();
                    out.push(Input::Segment { seq, outcome });
                }
            }
        }
        if let Some(mut open) = self.current.take() {
            match drain(&mut open) {
                Drained::Running(newest) => {
                    if let Some(text) = newest.filter(|_| !self.released)
                        && let Some(text) = sanitize_transcript(&text)
                    {
                        out.push(Input::StreamPartial {
                            seq: open.seq,
                            text,
                        });
                    }
                    self.current = Some(open);
                }
                Drained::Ended(outcome) if self.released => {
                    out.push(Input::Transcribed(outcome));
                    self.done = true;
                }
                // The engine ended this session on its own: a segment; the
                // next audio opens a new session.
                Drained::Ended(outcome) => out.push(Input::Segment {
                    seq: open.seq,
                    outcome,
                }),
            }
        }
        if self.released && !self.done && self.current.is_none() {
            out.push(Input::Transcribed(if self.failed {
                Outcome::Failed(STREAM_FAILED.into())
            } else {
                Outcome::Nothing
            }));
            self.done = true;
        }
        out
    }

    /// Key released: the rest of the audio goes in and the newest session
    /// finishes. Its final arrives as `Input::Transcribed` from `poll`.
    pub fn release(&mut self, rest: &[f32]) -> Vec<Input> {
        let raw = self.release_raw(rest);
        self.corrected(raw)
    }

    fn release_raw(&mut self, rest: &[f32]) -> Vec<Input> {
        if self.released || self.done {
            return Vec::new();
        }
        if let Some(open) = self.current.as_mut() {
            if open.session.push_audio(rest).is_ok() {
                open.finish_or_abandon();
            } else {
                open.abandon();
            }
        }
        self.released = true;
        self.poll_raw()
    }

    /// Esc or discard: every session stops, no further inputs.
    pub fn cancel(&mut self) {
        for mut open in self
            .current
            .take()
            .into_iter()
            .chain(self.closing.drain(..))
        {
            open.session.cancel();
        }
        self.released = true;
        self.done = true;
    }
}
