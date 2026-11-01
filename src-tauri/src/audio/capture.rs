use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};

use super::meter::calculate_rms_db;
use super::resample::{StreamResampler16k, downmix_to_mono, resample_to_16k};

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

/// Turns raw native-rate chunks into 16 kHz mono as they arrive, with the
/// same transient discard and length cap as `stop()`. Memory stays bounded:
/// output is handed out on every call, nothing is accumulated.
pub struct LiveDrain {
    channels: u16,
    resampler: StreamResampler16k,
    transient_left: usize,
    room_left: usize,
}

impl LiveDrain {
    pub fn new(
        sample_rate: u32,
        channels: u16,
        config: AudioCaptureConfig,
    ) -> Result<Self, String> {
        if channels == 0 {
            return Err("Channel count must be greater than 0".to_string());
        }
        let probe = InMemoryAudioBuffer::new(16000, config);
        Ok(Self {
            channels,
            resampler: StreamResampler16k::new(sample_rate)?,
            transient_left: probe.transient_samples_to_discard,
            room_left: probe.max_samples,
        })
    }

    fn limit(&mut self, mut out: Vec<f32>) -> Vec<f32> {
        let skip = self.transient_left.min(out.len());
        self.transient_left -= skip;
        out.drain(..skip);
        out.truncate(self.room_left);
        self.room_left -= out.len();
        out
    }

    /// Feeds one raw interleaved chunk; returns the new 16 kHz samples.
    pub fn push(&mut self, raw: &[f32]) -> Result<Vec<f32>, String> {
        let mono = downmix_to_mono(raw, self.channels)?;
        let out = self.resampler.push(&mono)?;
        Ok(self.limit(out))
    }

    /// Returns the last samples held inside the resampler.
    pub fn finish(&mut self) -> Result<Vec<f32>, String> {
        let out = self.resampler.finish()?;
        Ok(self.limit(out))
    }
}

/// Status of microphone permission on macOS / OS level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicPermissionStatus {
    Granted,
    Denied,
    Undetermined,
}

/// Maps the raw `AVAuthorizationStatus` integer values to `MicPermissionStatus`.
/// 0 NotDetermined, 1 Restricted, 2 Denied, 3 Authorized. Unknown values are Denied.
pub fn map_av_authorization_status(raw: isize) -> MicPermissionStatus {
    match raw {
        0 => MicPermissionStatus::Undetermined,
        3 => MicPermissionStatus::Granted,
        _ => MicPermissionStatus::Denied,
    }
}

/// Seam for the OS microphone permission query and request.
pub trait MicPermissionProvider {
    fn status(&self) -> MicPermissionStatus;

    /// Asks the OS for microphone access. `on_done(granted)` is called once, possibly on
    /// another thread, after the user answers (or immediately if already decided).
    fn request(&self, on_done: Box<dyn FnOnce(bool) + Send + 'static>);
}

/// Real macOS provider backed by AVFoundation (`AVCaptureDevice`).
#[cfg(target_os = "macos")]
pub struct AvFoundationProvider;

#[cfg(target_os = "macos")]
impl MicPermissionProvider for AvFoundationProvider {
    fn status(&self) -> MicPermissionStatus {
        use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
        // SAFETY: AVMediaTypeAudio is a constant NSString exported by AVFoundation.
        let media_type = unsafe { AVMediaTypeAudio }.expect("AVMediaTypeAudio is available");
        let raw = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };
        map_av_authorization_status(raw.0)
    }

    fn request(&self, on_done: Box<dyn FnOnce(bool) + Send + 'static>) {
        use block2::RcBlock;
        use objc2::runtime::Bool;
        use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
        use std::sync::Mutex;

        let media_type = unsafe { AVMediaTypeAudio }.expect("AVMediaTypeAudio is available");
        // The handler is an Fn block, so the FnOnce is parked in a Mutex<Option<_>>.
        let slot = Mutex::new(Some(on_done));
        let handler = RcBlock::new(move |granted: Bool| {
            if let Some(cb) = slot.lock().ok().and_then(|mut g| g.take()) {
                cb(granted.as_bool());
            }
        });
        unsafe {
            AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &handler);
        }
    }
}

/// Fallback provider for non-macOS: only probes for a default input device via cpal.
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

    fn request(&self, on_done: Box<dyn FnOnce(bool) + Send + 'static>) {
        on_done(self.status() == MicPermissionStatus::Granted);
    }
}

/// Checks microphone permission using the given provider.
pub fn check_microphone_permission_with(
    provider: &dyn MicPermissionProvider,
) -> MicPermissionStatus {
    provider.status()
}

/// Requests microphone permission using the given provider.
pub fn request_microphone_permission_with(
    provider: &dyn MicPermissionProvider,
    on_done: Box<dyn FnOnce(bool) + Send + 'static>,
) {
    provider.request(on_done)
}

/// Checks microphone permission with the platform provider (AVFoundation on macOS).
pub fn check_microphone_permission() -> MicPermissionStatus {
    #[cfg(target_os = "macos")]
    return check_microphone_permission_with(&AvFoundationProvider);
    #[cfg(not(target_os = "macos"))]
    check_microphone_permission_with(&CpalDeviceProbe)
}

/// Requests microphone permission with the platform provider.
pub fn request_microphone_permission(on_done: Box<dyn FnOnce(bool) + Send + 'static>) {
    #[cfg(target_os = "macos")]
    return request_microphone_permission_with(&AvFoundationProvider, on_done);
    #[cfg(not(target_os = "macos"))]
    request_microphone_permission_with(&CpalDeviceProbe, on_done)
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
    live: Option<LiveDrain>,
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
            live: None,
        })
    }

    /// Live mode: 16 kHz mono samples recorded since the last call. Once used,
    /// end the session with `finish_live`, not `stop`.
    pub fn drain_16k(&mut self) -> Result<Vec<f32>, String> {
        if self.live.is_none() {
            self.live = Some(LiveDrain::new(
                self.sample_rate,
                self.channels,
                self.config.clone(),
            )?);
        }
        let mut out = Vec::new();
        while let Ok(chunk) = self.receiver.try_recv() {
            if let Some(live) = self.live.as_mut() {
                out.extend(live.push(&chunk)?);
            }
        }
        Ok(out)
    }

    /// Stops a live session and returns the samples not drained yet.
    pub fn finish_live(mut self) -> Result<Vec<f32>, String> {
        self.running.store(false, Ordering::Relaxed);
        if let Some(stream) = self.stream.take() {
            let _ = stream.pause();
        }
        let mut out = self.drain_16k()?;
        if let Some(live) = self.live.as_mut() {
            out.extend(live.finish()?);
        }
        Ok(out)
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
