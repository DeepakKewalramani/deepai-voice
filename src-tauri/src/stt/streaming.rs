//! Streaming speech to text: audio goes in while the user speaks, partial text comes out.
//!
//! A [`StreamingStt`] engine opens a [`StreamingSession`] per dictation. The caller pushes
//! 16 kHz mono f32 chunks with [`StreamingSession::push_audio`], reads [`StreamEvent`]s from an
//! std mpsc receiver, and ends the stream with [`StreamingSession::finish`] (a `Final` or an
//! `Error` follows) or [`StreamingSession::cancel`] (no further events). No async runtime.
//!
//! Events carry transcript text. Callers must never log or persist it.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::trait_def::ProviderError;

/// One update from a streaming session.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Best guess so far for the whole utterance. Replaces any earlier partial.
    Partial(String),
    /// Final text for the whole utterance. No more events follow.
    Final(String),
    /// The session failed. No more events follow.
    Error(ProviderError),
}

/// A running streaming recognition. Dropping it without `finish` cancels it.
pub trait StreamingSession: Send {
    /// Appends 16 kHz mono f32 samples. Errors after `finish` or `cancel`.
    fn push_audio(&mut self, samples: &[f32]) -> Result<(), ProviderError>;

    /// Receiver for this session's events.
    fn events(&self) -> &Receiver<StreamEvent>;

    /// Marks the end of audio. A `Final` or `Error` event follows on [`Self::events`].
    fn finish(&mut self) -> Result<(), ProviderError>;

    /// Stops recognition. No further events are delivered.
    fn cancel(&mut self);
}

/// Options for one session. `Default` means no hints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartOptions {
    /// Phrases the engine should favour (dictionary words), at most 100.
    /// Never logged.
    pub hints: Vec<String>,
}

/// A streaming speech to text engine.
pub trait StreamingStt: Send + Sync {
    /// Opens a session for a BCP-47 locale such as "en-US".
    fn start(&self, locale: &str) -> Result<Box<dyn StreamingSession>, ProviderError>;

    /// Opens a session with options. Engines without hint support ignore
    /// them; the default does exactly what `start` does.
    fn start_with(
        &self,
        locale: &str,
        options: &StartOptions,
    ) -> Result<Box<dyn StreamingSession>, ProviderError> {
        let _ = options;
        self.start(locale)
    }

    /// Human-readable name used in logs.
    fn name(&self) -> &str;
}

/// Reads events until `Final` or `Error`, or until `timeout` passes. Partials are skipped.
pub fn collect_final(
    events: &Receiver<StreamEvent>,
    timeout: Duration,
) -> Result<String, ProviderError> {
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(StreamEvent::Partial(_)) => continue,
            Ok(StreamEvent::Final(text)) => return Ok(text),
            Ok(StreamEvent::Error(err)) => return Err(err),
            Err(RecvTimeoutError::Timeout) => return Err(ProviderError::Timeout),
            Err(RecvTimeoutError::Disconnected) => {
                return Err(ProviderError::Local(
                    "stream closed without a final result".into(),
                ));
            }
        }
    }
}

/// Test double. Each non-empty `push_audio` reveals the next scripted word as a `Partial`;
/// `finish` sends `Final` with every scripted word (or the configured error).
pub struct FakeStreamingStt {
    words: Vec<String>,
    start_error: Option<ProviderError>,
    finish_error: Option<ProviderError>,
    final_text: Option<String>,
    /// Hints of every `start_with` call, in order.
    hints_seen: Arc<Mutex<Vec<Vec<String>>>>,
}

impl FakeStreamingStt {
    pub fn new(words: &[&str]) -> Self {
        Self {
            words: words.iter().map(|w| w.to_string()).collect(),
            start_error: None,
            finish_error: None,
            final_text: None,
            hints_seen: Arc::default(),
        }
    }

    /// `finish` sends this text as `Final` instead of every scripted word
    /// (a revision of the last partial).
    pub fn with_final(mut self, text: &str) -> Self {
        self.final_text = Some(text.to_string());
        self
    }

    /// Hints received by each `start_with` call so far.
    pub fn hints_seen(&self) -> Vec<Vec<String>> {
        self.hints_seen
            .lock()
            .map(|h| h.clone())
            .unwrap_or_default()
    }

    /// `start` fails with this error.
    pub fn failing_start(mut self, err: ProviderError) -> Self {
        self.start_error = Some(err);
        self
    }

    /// `finish` delivers this error instead of `Final`.
    pub fn failing_finish(mut self, err: ProviderError) -> Self {
        self.finish_error = Some(err);
        self
    }
}

impl StreamingStt for FakeStreamingStt {
    fn start(&self, _locale: &str) -> Result<Box<dyn StreamingSession>, ProviderError> {
        if let Some(err) = &self.start_error {
            return Err(err.clone());
        }
        let (tx, rx) = mpsc::channel();
        Ok(Box::new(FakeSession {
            words: self.words.clone(),
            shown: 0,
            finish_error: self.finish_error.clone(),
            final_text: self.final_text.clone(),
            tx: Some(tx),
            rx,
        }))
    }

    fn start_with(
        &self,
        locale: &str,
        options: &StartOptions,
    ) -> Result<Box<dyn StreamingSession>, ProviderError> {
        if let Ok(mut seen) = self.hints_seen.lock() {
            seen.push(options.hints.clone());
        }
        self.start(locale)
    }

    fn name(&self) -> &str {
        "fake-streaming"
    }
}

struct FakeSession {
    words: Vec<String>,
    shown: usize,
    finish_error: Option<ProviderError>,
    final_text: Option<String>,
    tx: Option<Sender<StreamEvent>>,
    rx: Receiver<StreamEvent>,
}

impl StreamingSession for FakeSession {
    fn push_audio(&mut self, samples: &[f32]) -> Result<(), ProviderError> {
        let tx = self
            .tx
            .as_ref()
            .ok_or_else(|| ProviderError::BadRequest("session already ended".into()))?;
        if !samples.is_empty() && self.shown < self.words.len() {
            self.shown += 1;
            let _ = tx.send(StreamEvent::Partial(self.words[..self.shown].join(" ")));
        }
        Ok(())
    }

    fn events(&self) -> &Receiver<StreamEvent> {
        &self.rx
    }

    fn finish(&mut self) -> Result<(), ProviderError> {
        let tx = self
            .tx
            .take()
            .ok_or_else(|| ProviderError::BadRequest("session already ended".into()))?;
        let event = match self.finish_error.take() {
            Some(err) => StreamEvent::Error(err),
            None => StreamEvent::Final(
                self.final_text
                    .take()
                    .unwrap_or_else(|| self.words.join(" ")),
            ),
        };
        let _ = tx.send(event);
        Ok(())
    }

    fn cancel(&mut self) {
        self.tx = None;
    }
}
