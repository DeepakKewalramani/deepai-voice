//! Local Whisper (whisper.cpp via whisper-rs), CPU only.
//!
//! The model is loaded once into [`LocalWhisper`] and reused for every call.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

/// Sample rate Whisper expects.
pub const SAMPLE_RATE: usize = 16_000;

/// Default model path, relative to the repo root (dev builds).
pub const DEFAULT_MODEL_PATH: &str = "models/ggml-base.en.bin";

/// Errors from loading or running the model.
#[derive(Debug)]
pub enum WhisperError {
    ModelMissing(PathBuf),
    Load(String),
    Transcribe(String),
}

impl std::fmt::Display for WhisperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelMissing(p) => write!(f, "whisper model not found: {}", p.display()),
            Self::Load(e) => write!(f, "whisper model load failed: {e}"),
            Self::Transcribe(e) => write!(f, "whisper transcribe failed: {e}"),
        }
    }
}

impl std::error::Error for WhisperError {}

pub type Result<T> = std::result::Result<T, WhisperError>;

/// Text plus timing for one transcription.
#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub audio: Duration,
    pub elapsed: Duration,
}

/// Thread count for whisper: physical cores minus one (leave one for audio,
/// typing and the UI), at least 2, at most 8. Without a physical count, half
/// the logical cores stand in for it.
pub fn threads_for(physical: Option<usize>, logical: usize) -> usize {
    let cores = physical.unwrap_or(logical / 2).max(1);
    cores.saturating_sub(1).clamp(2, 8)
}

/// Physical core count from `sysctl hw.physicalcpu` (macOS), if readable.
fn physical_cores() -> Option<usize> {
    let out = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "hw.physicalcpu"])
        .output()
        .ok()?;
    String::from_utf8(out.stdout).ok()?.trim().parse().ok()
}

/// Picks the CPU thread count with `threads_for`.
pub fn default_threads() -> usize {
    let logical = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    threads_for(physical_cores(), logical)
}

/// Encoder frames per second of audio (1500 frames cover 30 s).
const CTX_PER_SECOND: usize = 50;
/// Smallest and largest audio context for a partial pass.
pub const MIN_AUDIO_CTX: i32 = 256;
pub const MAX_AUDIO_CTX: i32 = 1500;

/// Reduced encoder context for a partial pass over `samples` (16 kHz):
/// ceil(seconds * 50), at least 256, at most the full 1500. Whisper otherwise
/// encodes a full 30 s window, so a short partial costs as much as a long one.
pub fn partial_audio_ctx(samples: usize) -> i32 {
    let frames = (samples * CTX_PER_SECOND).div_ceil(SAMPLE_RATE);
    (frames.min(MAX_AUDIO_CTX as usize) as i32).max(MIN_AUDIO_CTX)
}

/// Longest vocabulary hint passed to the model, in tokens. Whisper keeps at
/// most half its 448 token text context for the prompt.
pub const MAX_HINT_TOKENS: usize = 200;

/// Token buffer size for `hint`: never smaller than the number of tokens a
/// byte-level tokenizer can produce (one per byte, plus a little slack).
pub fn token_buffer_len(hint: &str) -> usize {
    hint.len() + 16
}

/// The hint actually used: None for no hint or a blank one (no prompt at all,
/// exactly as before hints existed), and never text with a NUL character.
pub fn effective_hint(hint: Option<&str>) -> Option<&str> {
    hint.map(str::trim)
        .filter(|h| !h.is_empty() && !h.contains('\0'))
}

/// A loaded Whisper model and its reusable decode state.
pub struct LocalWhisper {
    ctx: WhisperContext,
    state: WhisperState,
    threads: usize,
    load_time: Duration,
    /// Last hint and its tokens, so a hold's hint is tokenized once.
    hint_tokens: Option<(String, Vec<i32>)>,
}

impl LocalWhisper {
    /// Loads the model once and runs a short warm-up pass.
    pub fn load(model_path: &Path, threads: usize) -> Result<Self> {
        if !model_path.is_file() {
            return Err(WhisperError::ModelMissing(model_path.to_path_buf()));
        }
        let path = model_path
            .to_str()
            .ok_or_else(|| WhisperError::Load("model path is not valid UTF-8".into()))?;
        // Route whisper.cpp's own stdout logging away: it prints decoded tokens,
        // and transcript words must not reach logs.
        whisper_rs::install_logging_hooks();
        let started = Instant::now();
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|e| WhisperError::Load(e.to_string()))?;
        let state = ctx
            .create_state()
            .map_err(|e| WhisperError::Load(e.to_string()))?;
        let mut engine = Self {
            ctx,
            state,
            threads: threads.max(1),
            load_time: Duration::ZERO,
            hint_tokens: None,
        };
        // Warm-up: one second of silence so the first real call is not cold.
        engine.transcribe_timed(&vec![0.0; SAMPLE_RATE])?;
        engine.load_time = started.elapsed();
        tracing::info!(
            "local_whisper event=model_loaded threads={} load_ms={}",
            engine.threads,
            engine.load_time.as_millis()
        );
        Ok(engine)
    }

    /// Time spent in [`LocalWhisper::load`], including warm-up.
    pub fn load_time(&self) -> Duration {
        self.load_time
    }

    /// Transcribes 16 kHz mono f32 samples to text.
    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String> {
        self.transcribe_timed(samples).map(|t| t.text)
    }

    /// Fast, less exact pass for live partials: reduced audio context
    /// (`partial_audio_ctx`). Finals keep the full context.
    pub fn transcribe_partial(&mut self, samples: &[f32]) -> Result<String> {
        self.transcribe_partial_with_hint(samples, None)
    }

    /// Like [`LocalWhisper::transcribe`], with timing. Logs lengths and durations only.
    pub fn transcribe_timed(&mut self, samples: &[f32]) -> Result<Transcript> {
        self.run(samples, None, None)
    }

    /// [`LocalWhisper::transcribe`] with an optional vocabulary hint (the
    /// initial prompt). `None` behaves exactly like `transcribe`.
    pub fn transcribe_with_hint(&mut self, samples: &[f32], hint: Option<&str>) -> Result<String> {
        self.run(samples, None, hint).map(|t| t.text)
    }

    /// [`LocalWhisper::transcribe_partial`] with an optional vocabulary hint.
    pub fn transcribe_partial_with_hint(
        &mut self,
        samples: &[f32],
        hint: Option<&str>,
    ) -> Result<String> {
        self.run(samples, Some(partial_audio_ctx(samples.len())), hint)
            .map(|t| t.text)
    }

    /// Tokens of `hint`, cached for the next call with the same hint. The
    /// same tokenizer whisper.cpp uses for its initial prompt; passing tokens
    /// avoids a per-call allocation that whisper-rs never frees.
    fn hint_tokens(&mut self, hint: Option<&str>) -> Result<Option<&[i32]>> {
        let Some(hint) = effective_hint(hint) else {
            return Ok(None);
        };
        if self.hint_tokens.as_ref().is_none_or(|(h, _)| h != hint) {
            // whisper.cpp returns minus the token count when the buffer is too
            // small and whisper-rs then treats that as a length. A byte-level
            // tokenizer never makes more tokens than bytes, so size the buffer
            // from the byte length and cut to the cap afterwards.
            let mut tokens = self
                .ctx
                .tokenize(hint, token_buffer_len(hint))
                .map_err(|e| WhisperError::Transcribe(e.to_string()))?;
            tokens.truncate(MAX_HINT_TOKENS);
            self.hint_tokens = Some((hint.to_string(), tokens));
        }
        Ok(self.hint_tokens.as_ref().map(|(_, t)| t.as_slice()))
    }

    fn run(
        &mut self,
        samples: &[f32],
        audio_ctx: Option<i32>,
        hint: Option<&str>,
    ) -> Result<Transcript> {
        // A hint that cannot be tokenized is dropped: transcribe without it.
        let tokens = self.hint_tokens(hint).ok().flatten().map(<[i32]>::to_vec);
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        if let Some(tokens) = tokens.as_deref() {
            params.set_tokens(tokens);
        }
        params.set_n_threads(self.threads as i32);
        if let Some(ctx) = audio_ctx {
            params.set_audio_ctx(ctx);
        }
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        let started = Instant::now();
        self.state
            .full(params, samples)
            .map_err(|e| WhisperError::Transcribe(e.to_string()))?;
        let mut text = String::new();
        for segment in self.state.as_iter() {
            let s = segment
                .to_str_lossy()
                .map_err(|e| WhisperError::Transcribe(e.to_string()))?;
            text.push_str(&s);
        }
        let text = text.trim().to_string();
        let elapsed = started.elapsed();
        let audio = Duration::from_secs_f64(samples.len() as f64 / SAMPLE_RATE as f64);
        tracing::info!(
            "local_whisper event=transcribed audio_ms={} elapsed_ms={} audio_ctx={} hint_tokens={} chars={}",
            audio.as_millis(),
            elapsed.as_millis(),
            audio_ctx.unwrap_or(0),
            tokens.as_ref().map_or(0, Vec::len),
            text.chars().count()
        );
        Ok(Transcript {
            text,
            audio,
            elapsed,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn token_buffer_covers_one_token_per_byte_so_the_library_never_sees_a_negative_count() {
        // 400 Devanagari characters are 1200 bytes; the old fixed buffer of
        // 200 tokens was far too small for that.
        let hint = "\u{0926}".repeat(400);
        assert!(super::token_buffer_len(&hint) >= hint.len());
        assert!(super::token_buffer_len("") >= 1);
    }

    use super::*;

    #[test]
    fn missing_model_is_a_clear_error() {
        let err = LocalWhisper::load(Path::new("no/such/model.bin"), 4)
            .err()
            .expect("must fail");
        assert!(matches!(err, WhisperError::ModelMissing(_)));
        assert!(err.to_string().contains("no/such/model.bin"));
    }

    #[test]
    fn threads_leave_one_core_free_with_a_minimum_of_two() {
        assert_eq!(threads_for(Some(8), 16), 7);
        assert_eq!(threads_for(Some(4), 8), 3);
        assert_eq!(threads_for(Some(2), 4), 2);
        assert_eq!(threads_for(Some(1), 1), 2);
        assert_eq!(threads_for(Some(24), 24), 8);
        assert_eq!(threads_for(None, 10), 4);
    }

    #[test]
    fn partial_context_scales_with_audio_length() {
        assert_eq!(partial_audio_ctx(0), MIN_AUDIO_CTX);
        assert_eq!(partial_audio_ctx(SAMPLE_RATE), MIN_AUDIO_CTX); // 1 s -> 50
        assert_eq!(partial_audio_ctx(SAMPLE_RATE * 6), 300);
        assert_eq!(partial_audio_ctx(SAMPLE_RATE * 6 + 1), 301);
        assert_eq!(partial_audio_ctx(SAMPLE_RATE * 30), MAX_AUDIO_CTX);
        assert_eq!(partial_audio_ctx(SAMPLE_RATE * 60), MAX_AUDIO_CTX);
    }

    #[test]
    fn no_hint_or_blank_hint_means_no_prompt_at_all() {
        assert_eq!(effective_hint(None), None);
        assert_eq!(effective_hint(Some("")), None);
        assert_eq!(effective_hint(Some("   ")), None);
        assert_eq!(effective_hint(Some("bad\0hint")), None);
        assert_eq!(
            effective_hint(Some(" Vocabulary: Deepak ")),
            Some("Vocabulary: Deepak")
        );
    }

    #[test]
    fn threads_are_clamped() {
        let n = default_threads();
        assert!((1..=8).contains(&n));
    }
}
