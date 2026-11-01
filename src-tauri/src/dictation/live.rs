//! Live dictation building blocks, all pure: an energy-based segmenter that
//! cuts the held-key audio into utterances, and the Whisper job queue that
//! runs finals before partials. Audio stays in memory and is never logged.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use super::Outcome;
use crate::dictionary::HoldVocab;
use crate::stt::filters::{MIN_DURATION_SAMPLES, sanitize_transcript};

/// 16 kHz mono, the rate the recorder hands out.
pub const SAMPLE_RATE: usize = 16_000;
const FRAME: usize = SAMPLE_RATE / 50; // 20 ms
const fn frames(ms: usize) -> usize {
    ms / 20
}
/// Lowest noise floor estimate (about -66 dBFS).
pub const MIN_FLOOR: f32 = 0.0005;
/// A frame is voice when its RMS is this many times the noise floor (+9 dB).
pub const VOICE_MARGIN: f32 = 2.8;
/// Voice threshold never goes below this (about -54 dBFS), so digital
/// silence plus a click is not speech.
pub const MIN_VOICE_RMS: f32 = 0.002;
/// Per-frame rise of the floor estimate: about x e every 3 s. It drops at
/// once to a quieter frame, so it tracks the quiet frames (a low percentile).
const FLOOR_RISE: f32 = 1.0 / 150.0;
/// Voice needed before a segment opens. 180 ms: a 150 ms blip stays ignored.
const ONSET_FRAMES: usize = frames(180);
/// Short dips inside the onset run do not reset it.
const ONSET_GAP_FRAMES: usize = frames(160);
/// Silence that ends a segment.
const END_PAUSE_FRAMES: usize = frames(600);
/// Silence kept at the end of a finished segment.
const KEEP_TAIL_FRAMES: usize = frames(200);
/// Audio kept from before the onset, so the first word is not clipped.
const PREROLL_FRAMES: usize = frames(200);
/// A segment is force-cut at this length. Also the partial window, so a
/// partial always covers the whole open segment.
pub const MAX_SEGMENT_SAMPLES: usize = 8 * SAMPLE_RATE;

/// A finished utterance.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// 1-based, rises by one per segment within a hold.
    pub seq: u64,
    pub samples: Vec<f32>,
}

#[derive(Debug)]
enum Phase {
    /// Waiting for speech. `lead` holds the onset run plus pre-roll.
    Waiting {
        lead: VecDeque<f32>,
        voiced: usize,
        gap: usize,
    },
    Open {
        audio: Vec<f32>,
        silent: usize,
    },
}

/// Energy-based utterance segmenter over 20 ms frames.
#[derive(Debug)]
pub struct Segmenter {
    phase: Phase,
    frame: Vec<f32>,
    seq: u64,
    gate: NoiseFloor,
}

impl Default for Segmenter {
    fn default() -> Self {
        Self::new()
    }
}

fn waiting() -> Phase {
    Phase::Waiting {
        lead: VecDeque::new(),
        voiced: 0,
        gap: 0,
    }
}

pub fn frame_rms(f: &[f32]) -> f32 {
    if f.is_empty() {
        return 0.0;
    }
    (f.iter().map(|s| s * s).sum::<f32>() / f.len() as f32).sqrt()
}

/// Adaptive voice gate: tracks the room's noise floor from frame RMS and
/// calls a frame voice when it is `VOICE_MARGIN` above it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoiseFloor {
    floor: f32,
}

impl Default for NoiseFloor {
    fn default() -> Self {
        Self { floor: MIN_FLOOR }
    }
}

impl NoiseFloor {
    /// Starts from a floor learned earlier (for example the previous hold).
    pub fn with_floor(floor: f32) -> Self {
        Self {
            floor: floor.max(MIN_FLOOR),
        }
    }

    pub fn floor(&self) -> f32 {
        self.floor
    }

    pub fn threshold(&self) -> f32 {
        (self.floor * VOICE_MARGIN).max(MIN_VOICE_RMS)
    }

    /// Classifies one 20 ms frame, then updates the floor.
    pub fn is_voice(&mut self, rms: f32) -> bool {
        let voice = rms >= self.threshold();
        self.floor = rms.min(self.floor * (1.0 + FLOOR_RISE)).max(MIN_FLOOR);
        voice
    }
}

/// A hold this long with no segment gets one whole-audio Whisper pass.
pub const SAFETY_NET_MIN_SAMPLES: usize = SAMPLE_RATE * 2 / 5;
/// RMS at or below this is digital silence: no safety net pass.
pub const SAFETY_NET_MIN_RMS: f32 = 0.0001;

/// True when the segmenter found no speech at all in a hold that lasted at
/// least 0.4 s and is not pure digital silence.
pub fn safety_net_wanted(segments: u64, audio: &[f32]) -> bool {
    segments == 0 && audio.len() >= SAFETY_NET_MIN_SAMPLES && frame_rms(audio) > SAFETY_NET_MIN_RMS
}

/// Shortest gap between partial passes.
pub const PARTIAL_MIN_GAP: Duration = Duration::from_millis(300);

/// A new partial is due when `since_last` covers both the minimum gap and
/// the cost of the last partial pass, so slow passes never pile up.
pub fn partial_due(since_last: Duration, last_cost: Duration) -> bool {
    since_last >= PARTIAL_MIN_GAP.max(last_cost)
}

impl Segmenter {
    pub fn new() -> Self {
        Self::with_floor(MIN_FLOOR)
    }

    /// Starts with a noise floor learned in an earlier hold.
    pub fn with_floor(floor: f32) -> Self {
        Self {
            phase: waiting(),
            frame: Vec::with_capacity(FRAME),
            seq: 0,
            gate: NoiseFloor::with_floor(floor),
        }
    }

    /// Current noise floor estimate, to carry into the next hold.
    pub fn floor(&self) -> f32 {
        self.gate.floor()
    }

    /// Segments opened so far in this hold.
    pub fn segments(&self) -> u64 {
        self.seq
    }

    pub fn is_open(&self) -> bool {
        matches!(self.phase, Phase::Open { .. })
    }

    /// Feeds 16 kHz mono audio; returns the segments it finished.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Segment> {
        let mut done = Vec::new();
        for &s in samples {
            self.frame.push(s);
            if self.frame.len() == FRAME {
                let frame = std::mem::take(&mut self.frame);
                if let Some(seg) = self.step(&frame) {
                    done.push(seg);
                }
                self.frame = frame;
                self.frame.clear();
            }
        }
        done
    }

    fn step(&mut self, frame: &[f32]) -> Option<Segment> {
        let voice = self.gate.is_voice(frame_rms(frame));
        match &mut self.phase {
            Phase::Waiting { lead, voiced, gap } => {
                lead.extend(frame);
                if voice {
                    *voiced += 1;
                    *gap = 0;
                } else {
                    *gap += 1;
                    if *gap > ONSET_GAP_FRAMES {
                        *voiced = 0;
                    }
                }
                if *voiced >= ONSET_FRAMES {
                    self.seq += 1;
                    self.phase = Phase::Open {
                        audio: lead.drain(..).collect(),
                        silent: 0,
                    };
                } else {
                    // Keep the onset window plus pre-roll, nothing older.
                    let keep = (ONSET_FRAMES + ONSET_GAP_FRAMES + PREROLL_FRAMES) * FRAME;
                    while lead.len() > keep {
                        lead.pop_front();
                    }
                }
                None
            }
            Phase::Open { audio, silent } => {
                audio.extend_from_slice(frame);
                *silent = if voice { 0 } else { *silent + 1 };
                if *silent >= END_PAUSE_FRAMES {
                    let cut = (*silent - KEEP_TAIL_FRAMES) * FRAME;
                    audio.truncate(audio.len() - cut);
                    return self.finish();
                }
                if audio.len() >= MAX_SEGMENT_SAMPLES {
                    return self.finish();
                }
                None
            }
        }
    }

    fn finish(&mut self) -> Option<Segment> {
        match std::mem::replace(&mut self.phase, waiting()) {
            Phase::Open { audio, .. } => Some(Segment {
                seq: self.seq,
                samples: audio,
            }),
            Phase::Waiting { .. } => None,
        }
    }

    /// Ends the hold: the open segment, if any, as a final. Waiting audio
    /// that never reached the onset is dropped.
    pub fn flush(&mut self) -> Option<Segment> {
        if let Phase::Open { audio, .. } = &mut self.phase {
            audio.append(&mut self.frame);
        }
        self.frame.clear();
        self.finish()
    }

    /// Copy of the open segment's audio for a partial pass.
    pub fn open_audio(&self) -> Option<(u64, Vec<f32>)> {
        match &self.phase {
            Phase::Open { audio, .. } => Some((self.seq, audio.clone())),
            Phase::Waiting { .. } => None,
        }
    }
}

/// Whisper work for the live hold.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    /// A finished segment. `last` marks the release tail.
    Final {
        seq: u64,
        samples: Vec<f32>,
        last: bool,
    },
    /// The open segment so far, for the live preview and typing.
    Partial { seq: u64, samples: Vec<f32> },
}

/// Finals run in order and before any partial; only the newest partial is
/// kept, and a final drops a partial of the same or an older segment.
#[derive(Debug, Default)]
pub struct JobQueue {
    finals: VecDeque<Job>,
    partial: Option<(u64, Vec<f32>)>,
    running: bool,
    /// Hold that queued jobs belong to.
    hold: u64,
    /// How long the last partial pass took.
    partial_cost: Duration,
    /// Dictionary of the hold, read once at hold start.
    vocab: Arc<HoldVocab>,
}

impl JobQueue {
    pub fn push_final(&mut self, seq: u64, samples: Vec<f32>, last: bool) {
        if matches!(self.partial, Some((p, _)) if p <= seq) {
            self.partial = None;
        }
        self.finals.push_back(Job::Final { seq, samples, last });
    }

    pub fn push_partial(&mut self, seq: u64, samples: Vec<f32>) {
        self.partial = Some((seq, samples));
    }

    /// Next job to run; the queue counts as busy until `done`.
    pub fn take_next(&mut self) -> Option<Job> {
        let job = self.finals.pop_front().or_else(|| {
            self.partial
                .take()
                .map(|(seq, samples)| Job::Partial { seq, samples })
        });
        if job.is_some() {
            self.running = true;
        }
        job
    }

    pub fn done(&mut self) {
        self.running = false;
    }

    /// Records the cost of a finished partial pass (adaptive cadence).
    pub fn set_partial_cost(&mut self, cost: Duration) {
        self.partial_cost = cost;
    }

    pub fn partial_cost(&self) -> Duration {
        self.partial_cost
    }

    /// Nothing queued and nothing running.
    pub fn is_idle(&self) -> bool {
        !self.running && self.finals.is_empty() && self.partial.is_none()
    }

    /// Drops all queued work (a discarded hold). A running job finishes.
    pub fn clear(&mut self) {
        self.finals.clear();
        self.partial = None;
    }

    /// A new hold starts: drops all queued work of earlier holds. Jobs taken
    /// from now on belong to `hold`; a job already running keeps its old id.
    pub fn begin_hold(&mut self, hold: u64) {
        self.begin_hold_with(hold, Arc::default());
    }

    /// Like `begin_hold`, with the hold's dictionary (`vocab`).
    pub fn begin_hold_with(&mut self, hold: u64, vocab: Arc<HoldVocab>) {
        self.clear();
        self.hold = hold;
        self.vocab = vocab;
    }

    /// Dictionary of the queued jobs' hold. Read it with `take_next`.
    pub fn vocab(&self) -> Arc<HoldVocab> {
        self.vocab.clone()
    }

    /// Hold of the jobs in the queue. Read it together with `take_next`
    /// (same lock) to tag the job's result.
    pub fn hold(&self) -> u64 {
        self.hold
    }
}

/// Transcribes one segment. The segmenter already found speech, so there is
/// no silence gate; short audio is padded with silence to the minimum length.
pub fn process_segment(
    samples: &[f32],
    transcribe: impl FnOnce(&[f32]) -> Result<String, String>,
) -> Outcome {
    if samples.is_empty() {
        return Outcome::Nothing;
    }
    let mut padded;
    let audio = if samples.len() < MIN_DURATION_SAMPLES {
        padded = samples.to_vec();
        padded.resize(MIN_DURATION_SAMPLES, 0.0);
        &padded[..]
    } else {
        samples
    };
    match transcribe(audio) {
        Ok(raw) => match sanitize_transcript(&raw) {
            Some(text) => Outcome::Text(text),
            None => Outcome::Nothing,
        },
        Err(msg) => Outcome::Failed(msg),
    }
}
