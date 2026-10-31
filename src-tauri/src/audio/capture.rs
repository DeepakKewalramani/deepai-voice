use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};

use super::meter::calculate_rms_db;
use super::resample::{downmix_to_mono, resample_to_16k};

/// Configuration parameters for audio capture.
#[derive(Debug, Clone)]
pub struct AudioCaptureConfig {
    pub discard_transient_ms: u32,
    pub max_record_seconds: u32,
}

impl Default for AudioCaptureConfig {
    fn default() -> Self {
        Self {
            discard_transient_ms: 50,
            max_record_seconds: 120,
        }
    }
}

/// In-memory buffer managing audio samples, transient discarding, and length capping.
/// NEVER writes audio to disk.
pub struct InMemoryAudioBuffer {
    _sample_rate: u32,
    _config: AudioCaptureConfig,
    samples: Vec<f32>,
    transient_samples_to_discard: usize,
    max_samples: usize,
    is_capped: bool,
}

impl InMemoryAudioBuffer {
    pub fn new(sample_rate: u32, config: AudioCaptureConfig) -> Self {
        let transient_samples =
            (sample_rate as f64 * (config.discard_transient_ms as f64 / 1000.0)).round() as usize;
        let max_samples = (sample_rate as f64 * config.max_record_seconds as f64).round() as usize;

        Self {
            _sample_rate: sample_rate,
            _config: config,
            samples: Vec::new(),
            transient_samples_to_discard: transient_samples,
            max_samples,
            is_capped: false,
        }
    }

    /// Push audio samples, discarding initial transient and capping at max length.
    pub fn push_samples(&mut self, incoming: &[f32]) {
        if self.is_capped || incoming.is_empty() {
            return;
        }

        let mut slice = incoming;

        // Discard initial transient if still pending
        if self.transient_samples_to_discard > 0 {
            if slice.len() <= self.transient_samples_to_discard {
                self.transient_samples_to_discard -= slice.len();
                return;
            } else {
                slice = &slice[self.transient_samples_to_discard..];
                self.transient_samples_to_discard = 0;
            }
        }

        let remaining_space = self.max_samples.saturating_sub(self.samples.len());
        if slice.len() >= remaining_space {
            self.samples.extend_from_slice(&slice[..remaining_space]);
            self.is_capped = true;
        } else {
            self.samples.extend_from_slice(slice);
        }
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn is_capped(&self) -> bool {
        self.is_capped
    }

    pub fn into_samples(self) -> Vec<f32> {
        self.samples
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
}

/// Status of microphone permission on macOS / OS level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicPermissionStatus {
    Granted,
    Denied,
    Undetermined,
}

/// Seam for the OS microphone permission query. A real AVFoundation
/// (`AVCaptureDevice authorizationStatusForMediaType`) provider can implement this
/// without touching callers. Needs the objc2 family of crates, not added yet.
pub trait MicPermissionProvider {
    fn status(&self) -> MicPermissionStatus;
}

/// Fallback provider: only probes for a default input device via cpal.
/// It cannot tell Denied from Undetermined, so a device present reports Granted
/// and a missing device reports Denied. This is NOT the real OS permission state.
pub struct CpalDeviceProbe;

impl MicPermissionProvider for CpalDeviceProbe {
    fn status(&self) -> MicPermissionStatus {
        match cpal::default_host().default_input_device() {
            Some(_) => MicPermissionStatus::Granted,
            None => MicPermissionStatus::Denied,
        }
    }
}

/// Checks microphone permission using the given provider.
pub fn check_microphone_permission_with(
    provider: &dyn MicPermissionProvider,
) -> MicPermissionStatus {
    provider.status()
}

/// Checks microphone availability / permission with the default (device probe) provider.
pub fn check_microphone_permission() -> MicPermissionStatus {
    check_microphone_permission_with(&CpalDeviceProbe)
}

/// Event emitted during live recording for UI visualization.
#[derive(Debug, Clone)]
pub struct LevelMeterEvent {
    pub rms_db: f32,
    pub is_silent: bool,
}

/// Subscriber for live level events (called from the audio thread; keep it cheap).
pub type LevelCallback = Arc<dyn Fn(LevelMeterEvent) + Send + Sync>;

/// RMS below this dBFS counts as silent.
const SILENCE_DB: f32 = -60.0;

/// Per-callback handler: bounds raw (native rate, interleaved) memory and emits levels.
/// Never touches disk.
pub struct RawChunkSink {
    remaining: usize,
    on_level: Option<LevelCallback>,
}

impl RawChunkSink {
    /// `max_raw_samples` is the cap in interleaved samples at native rate.
    pub fn new(max_raw_samples: usize, on_level: Option<LevelCallback>) -> Self {
        Self {
            remaining: max_raw_samples,
            on_level,
        }
    }

    /// Emits a level event for the chunk and returns the part that fits under the cap,
    /// or None when the cap is already reached.
    pub fn handle(&mut self, data: &[f32]) -> Option<Vec<f32>> {
        if data.is_empty() {
            return None;
        }
        if let Some(cb) = &self.on_level {
            let rms_db = calculate_rms_db(data);
            cb(LevelMeterEvent {
                rms_db,
                is_silent: rms_db < SILENCE_DB,
            });
        }
        if self.remaining == 0 {
            return None;
        }
        let take = data.len().min(self.remaining);
        self.remaining -= take;
        Some(data[..take].to_vec())
    }
}

/// Active audio capture controller.
pub struct AudioCaptureSession {
    running: Arc<AtomicBool>,
    stream: Option<Stream>,
    receiver: Receiver<Vec<f32>>,
    sample_rate: u32,
    channels: u16,
    config: AudioCaptureConfig,
}

impl AudioCaptureSession {
    /// Starts capturing audio from the default input device into memory.
    pub fn start(config: AudioCaptureConfig) -> Result<Self, String> {
        Self::start_with_level(config, None)
    }

    /// Like `start`, additionally emitting a `LevelMeterEvent` per audio chunk.
    pub fn start_with_level(
        config: AudioCaptureConfig,
        on_level: Option<LevelCallback>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "No default audio input device found".to_string())?;

        let default_config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get default input format: {:?}", e))?;

        let sample_rate = default_config.sample_rate();
        let channels = default_config.channels();
        let sample_format = default_config.sample_format();
        let stream_config = default_config.into();

        let (tx, rx) = channel::<Vec<f32>>();
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        // Raw cap: max seconds at native rate, interleaved. Transient discard happens at stop.
        let max_raw =
            sample_rate as usize * channels as usize * (config.max_record_seconds as usize + 1);
        let mut sink = RawChunkSink::new(max_raw, on_level);

        let err_fn = |err| {
            tracing::error!("CPAL input stream error: {:?}", err);
        };

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                stream_config,
                move |data: &[f32], _: &_| {
                    if running_clone.load(Ordering::Relaxed)
                        && let Some(c) = sink.handle(data)
                    {
                        let _ = tx.send(c);
                    }
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                stream_config,
                move |data: &[i16], _: &_| {
                    if running_clone.load(Ordering::Relaxed) {
                        let f32_data: Vec<f32> =
                            data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                        if let Some(c) = sink.handle(&f32_data) {
                            let _ = tx.send(c);
                        }
                    }
                },
                err_fn,
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                stream_config,
                move |data: &[u16], _: &_| {
                    if running_clone.load(Ordering::Relaxed) {
                        let f32_data: Vec<f32> = data
                            .iter()
                            .map(|&s| (s as f32 - 32768.0) / 32768.0)
                            .collect();
                        if let Some(c) = sink.handle(&f32_data) {
                            let _ = tx.send(c);
                        }
                    }
                },
                err_fn,
                None,
            ),
            other => return Err(format!("Unsupported sample format: {:?}", other)),
        }
        .map_err(|e| format!("Failed to build input stream: {:?}", e))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start input stream: {:?}", e))?;

        Ok(Self {
            running,
            stream: Some(stream),
            receiver: rx,
            sample_rate,
            channels,
            config,
        })
    }

    /// Stops audio capture and returns 16 kHz mono float PCM samples held strictly in memory.
    pub fn stop(mut self) -> Result<Vec<f32>, String> {
        self.running.store(false, Ordering::Relaxed);
        if let Some(stream) = self.stream.take() {
            let _ = stream.pause();
        }

        // Collect all chunks from the channel
        let mut raw_chunks = Vec::new();
        while let Ok(chunk) = self.receiver.try_recv() {
            raw_chunks.extend_from_slice(&chunk);
        }

        if raw_chunks.is_empty() {
            return Ok(Vec::new());
        }

        // 1. Downmix to mono
        let mono = downmix_to_mono(&raw_chunks, self.channels)?;

        // 2. Resample to 16 kHz (whole buffer; bounded by the raw cap above, about 121 s)
        let resampled_16k = resample_to_16k(&mono, self.sample_rate)?;

        // 3. Apply memory buffer transient discard and length cap
        let mut buffer = InMemoryAudioBuffer::new(16000, self.config);
        buffer.push_samples(&resampled_16k);

        Ok(buffer.into_samples())
    }
}
