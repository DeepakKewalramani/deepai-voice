use std::f32::consts::PI;

use deepai_voice_lib::audio::meter::{calculate_peak_db, calculate_rms, calculate_rms_db};
use deepai_voice_lib::audio::resample::{downmix_to_mono, resample_to_16k};
use std::sync::{Arc, Mutex};

use deepai_voice_lib::audio::{
    AudioCaptureConfig, InMemoryAudioBuffer, LevelMeterEvent, MicPermissionProvider,
    MicPermissionStatus, RawChunkSink, check_microphone_permission_with,
};

#[test]
fn test_downmix_to_mono_stereo() {
    // Stereo interleaved [L0, R0, L1, R1, ...]
    let interleaved = vec![1.0, 0.0, 0.5, 0.5, -0.5, 0.5];
    let mono = downmix_to_mono(&interleaved, 2).expect("downmix stereo");
    assert_eq!(mono.len(), 3);
    assert!((mono[0] - 0.5).abs() < 1e-6);
    assert!((mono[1] - 0.5).abs() < 1e-6);
    assert!((mono[2] - 0.0).abs() < 1e-6);
}

#[test]
fn test_downmix_to_mono_already_mono() {
    let input = vec![0.1, 0.2, 0.3];
    let mono = downmix_to_mono(&input, 1).expect("downmix mono");
    assert_eq!(mono, input);
}

#[test]
fn test_downmix_invalid_channel_count() {
    let input = vec![0.1, 0.2];
    assert!(downmix_to_mono(&input, 0).is_err());
}

#[test]
fn test_calculate_rms_and_dbs() {
    let empty: [f32; 0] = [];
    assert_eq!(calculate_rms(&empty), 0.0);
    assert_eq!(calculate_rms_db(&empty), -100.0);

    // DC signal with amplitude 0.5
    let dc = vec![0.5; 100];
    let rms = calculate_rms(&dc);
    assert!((rms - 0.5).abs() < 1e-5);

    // Peak dB for 0.5 amplitude -> 20 * log10(0.5) = approx -6.02 dB
    let peak_db = calculate_peak_db(&dc);
    assert!((peak_db - (-6.0206)).abs() < 0.05);

    // Silent signal
    let zeros = vec![0.0; 100];
    assert_eq!(calculate_rms(&zeros), 0.0);
    assert_eq!(calculate_rms_db(&zeros), -100.0);
    assert_eq!(calculate_peak_db(&zeros), -100.0);
}

#[test]
fn test_resample_48k_to_16k_sine_wave() {
    // 48 kHz sine wave at 440 Hz for 0.1s (4800 samples)
    let in_sample_rate = 48000;
    let duration_s = 0.1;
    let n_samples = (in_sample_rate as f32 * duration_s) as usize;
    let freq = 440.0;

    let input: Vec<f32> = (0..n_samples)
        .map(|i| {
            let t = i as f32 / in_sample_rate as f32;
            (2.0 * PI * freq * t).sin()
        })
        .collect();

    let resampled = resample_to_16k(&input, in_sample_rate).expect("resample 48k to 16k");

    // Output should be roughly 1600 samples (0.1s * 16000)
    let expected_len = (16000.0 * duration_s) as usize;
    let diff = (resampled.len() as isize - expected_len as isize).abs();
    assert!(
        diff < 50,
        "Expected ~{}, got {}",
        expected_len,
        resampled.len()
    );

    // Signal energy should be preserved within a tolerance
    let in_rms = calculate_rms(&input);
    let out_rms = calculate_rms(&resampled);
    assert!((in_rms - out_rms).abs() < 0.1);

    // Frequency check: zero crossings of the output must match 440 Hz at 16 kHz.
    // Skip edges where the resampler settles.
    let body = &resampled[100..resampled.len() - 100];
    let crossings = body
        .windows(2)
        .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
        .count();
    let secs = body.len() as f32 / 16000.0;
    let est_hz = crossings as f32 / 2.0 / secs;
    assert!((est_hz - freq).abs() < 15.0, "estimated {} Hz", est_hz);

    // Amplitude check: peak stays near 1.0 in the settled region.
    let peak = body.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
    assert!((peak - 1.0).abs() < 0.1, "peak {}", peak);
}

#[test]
fn test_resample_already_16k() {
    let input = vec![0.1, -0.2, 0.3, -0.4];
    let resampled = resample_to_16k(&input, 16000).expect("pass-through 16k");
    assert_eq!(resampled, input);
}

#[test]
fn test_in_memory_buffer_transient_discard_and_cap() {
    let config = AudioCaptureConfig {
        discard_transient_ms: 50,
        max_record_seconds: 2, // 2 seconds cap for test
    };

    let mut buf = InMemoryAudioBuffer::new(16000, config);

    // Initial 50 ms at 16 kHz is 800 samples.
    // Feed 1000 samples.
    let chunk1 = vec![0.5; 1000];
    buf.push_samples(&chunk1);

    // Only 1000 - 800 = 200 samples should remain
    assert_eq!(buf.len(), 200);

    // Now feed another 2 seconds (32000 samples)
    // Total added after transient = 200 + 32000 = 32200
    // Max cap is 2 seconds = 32000 samples.
    let chunk2 = vec![0.8; 32000];
    buf.push_samples(&chunk2);

    assert_eq!(buf.len(), 32000);
    assert!(buf.is_capped());

    // Get samples
    let samples = buf.into_samples();
    assert_eq!(samples.len(), 32000);
}

#[test]
fn test_in_memory_buffer_zero_disk_writes() {
    // Pushing audio must not create any file in the temp dir or the working dir.
    // (Source also has no std::fs usage under src/audio; checked in the next test.)
    let list = |dir: &std::path::Path| -> std::collections::BTreeSet<std::ffi::OsString> {
        std::fs::read_dir(dir)
            .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name()).collect())
            .unwrap_or_default()
    };
    let tmp = std::env::temp_dir();
    let cwd = std::env::current_dir().unwrap();
    let (tmp_before, cwd_before) = (list(&tmp), list(&cwd));

    let config = AudioCaptureConfig {
        discard_transient_ms: 0,
        max_record_seconds: 120,
    };
    let mut buf = InMemoryAudioBuffer::new(16000, config);
    buf.push_samples(&[0.1, 0.2, 0.3]);
    assert_eq!(buf.into_samples().len(), 3);

    // Other tests may run concurrently; only audio-looking new files count.
    let new_audio = |before: &std::collections::BTreeSet<std::ffi::OsString>,
                     dir: &std::path::Path| {
        list(dir).difference(before).any(|n| {
            let n = n.to_string_lossy().to_lowercase();
            n.ends_with(".wav") || n.ends_with(".raw") || n.ends_with(".pcm") || n.contains("audio")
        })
    };
    assert!(!new_audio(&tmp_before, &tmp));
    assert!(!new_audio(&cwd_before, &cwd));
}

#[test]
fn test_audio_module_has_no_fs_apis() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/audio");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let src = std::fs::read_to_string(&path).unwrap();
        for banned in ["std::fs", "File::create", "tempfile", "hound"] {
            assert!(!src.contains(banned), "{:?} uses {}", path, banned);
        }
    }
}

#[test]
fn test_raw_sink_bounds_memory_during_recording() {
    // Cap of 1000 raw samples; feed 10 x 400 samples.
    let mut sink = RawChunkSink::new(1000, None);
    let mut kept = 0;
    for _ in 0..10 {
        if let Some(c) = sink.handle(&[0.1; 400]) {
            kept += c.len();
        }
    }
    assert_eq!(kept, 1000);
    assert!(sink.handle(&[0.1; 10]).is_none());
}

#[test]
fn test_raw_sink_emits_level_events_even_past_cap() {
    let events: Arc<Mutex<Vec<LevelMeterEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let ev = events.clone();
    let mut sink = RawChunkSink::new(100, Some(Arc::new(move |e| ev.lock().unwrap().push(e))));
    sink.handle(&[0.5; 100]);
    sink.handle(&[0.0; 100]);
    let got = events.lock().unwrap();
    assert_eq!(got.len(), 2);
    assert!((got[0].rms_db - (-6.02)).abs() < 0.1);
    assert!(!got[0].is_silent);
    assert!(got[1].is_silent);
}

#[test]
fn test_permission_provider_seam() {
    struct Fake(MicPermissionStatus);
    impl MicPermissionProvider for Fake {
        fn status(&self) -> MicPermissionStatus {
            self.0
        }
        fn request(&self, on_done: Box<dyn FnOnce(bool) + Send + 'static>) {
            on_done(self.0 == MicPermissionStatus::Granted);
        }
    }
    for st in [
        MicPermissionStatus::Granted,
        MicPermissionStatus::Denied,
        MicPermissionStatus::Undetermined,
    ] {
        assert_eq!(check_microphone_permission_with(&Fake(st)), st);
    }
}

#[test]
fn test_av_status_mapping() {
    use deepai_voice_lib::audio::map_av_authorization_status as m;
    assert_eq!(m(0), MicPermissionStatus::Undetermined);
    assert_eq!(m(1), MicPermissionStatus::Denied); // Restricted
    assert_eq!(m(2), MicPermissionStatus::Denied);
    assert_eq!(m(3), MicPermissionStatus::Granted);
    assert_eq!(m(99), MicPermissionStatus::Denied);
}

#[test]
fn test_permission_request_through_seam() {
    use deepai_voice_lib::audio::request_microphone_permission_with;
    struct Fake(MicPermissionStatus);
    impl MicPermissionProvider for Fake {
        fn status(&self) -> MicPermissionStatus {
            self.0
        }
        fn request(&self, on_done: Box<dyn FnOnce(bool) + Send + 'static>) {
            on_done(self.0 == MicPermissionStatus::Granted);
        }
    }
    let got = Arc::new(Mutex::new(None));
    let g = got.clone();
    request_microphone_permission_with(
        &Fake(MicPermissionStatus::Granted),
        Box::new(move |ok| *g.lock().unwrap() = Some(ok)),
    );
    assert_eq!(*got.lock().unwrap(), Some(true));
}

// Live (streaming) drain: the same audio as `stop()`, delivered in pieces.

fn sine(rate: u32, secs: f32, freq: f32, channels: u16) -> Vec<f32> {
    let n = (rate as f32 * secs) as usize;
    let mut out = Vec::with_capacity(n * channels as usize);
    for i in 0..n {
        let s = (2.0 * PI * freq * i as f32 / rate as f32).sin() * 0.5;
        for _ in 0..channels {
            out.push(s);
        }
    }
    out
}

#[test]
fn test_stream_resampler_keeps_length_and_tone_in_any_chunking() {
    use deepai_voice_lib::audio::resample::StreamResampler16k;
    let input = sine(48000, 1.0, 440.0, 1);
    for chunk in [333usize, 1024, 4800, 48000] {
        let mut r = StreamResampler16k::new(48000).unwrap();
        let mut out = Vec::new();
        for c in input.chunks(chunk) {
            out.extend(r.push(c).unwrap());
        }
        out.extend(r.finish().unwrap());
        assert!(
            (out.len() as isize - 16000).abs() <= 2,
            "chunk {chunk}: {} samples",
            out.len()
        );
        let body = &out[200..out.len() - 200];
        assert!((calculate_rms(body) - 0.5 / 2f32.sqrt()).abs() < 0.02);
        // Same audio as the whole-buffer path.
        let whole = resample_to_16k(&input, 48000).unwrap();
        let worst = body
            .iter()
            .zip(&whole[200..])
            .fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
        assert!(worst < 0.01, "chunk {chunk}: differs by {worst}");
    }
}

#[test]
fn test_stream_resampler_returns_audio_before_finish() {
    use deepai_voice_lib::audio::resample::StreamResampler16k;
    let mut r = StreamResampler16k::new(44100).unwrap();
    let got = r.push(&sine(44100, 0.5, 300.0, 1)).unwrap();
    assert!(got.len() > 6000, "only {} samples after 0.5 s", got.len());
    let mut r = StreamResampler16k::new(16000).unwrap();
    assert_eq!(r.push(&[0.1, 0.2]).unwrap(), vec![0.1, 0.2]);
    assert!(r.finish().unwrap().is_empty());
}

#[test]
fn test_live_drain_matches_stop_path_rules() {
    use deepai_voice_lib::audio::LiveDrain;
    let config = AudioCaptureConfig {
        discard_transient_ms: 50,
        max_record_seconds: 1,
    };
    // 16 kHz stereo, 2 s: downmixed, first 50 ms dropped, capped at 1 s.
    let raw = sine(16000, 2.0, 200.0, 2);
    let mut live = LiveDrain::new(16000, 2, config.clone()).unwrap();
    let mut out = Vec::new();
    for c in raw.chunks(998) {
        out.extend(live.push(c).unwrap());
    }
    out.extend(live.finish().unwrap());

    let mono = downmix_to_mono(&raw, 2).unwrap();
    let mut expected = InMemoryAudioBuffer::new(16000, config);
    expected.push_samples(&mono);
    assert_eq!(out, expected.into_samples());
}
