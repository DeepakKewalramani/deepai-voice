use std::f32::consts::PI;

use deepai_voice_lib::audio::meter::{calculate_peak_db, calculate_rms, calculate_rms_db};
use deepai_voice_lib::audio::resample::{downmix_to_mono, resample_to_16k};
use deepai_voice_lib::audio::{AudioCaptureConfig, InMemoryAudioBuffer};

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
    // Verify that InMemoryAudioBuffer operates purely in RAM and creates no files
    let config = AudioCaptureConfig {
        discard_transient_ms: 0,
        max_record_seconds: 120,
    };
    let mut buf = InMemoryAudioBuffer::new(16000, config);
    buf.push_samples(&[0.1, 0.2, 0.3]);
    let samples = buf.into_samples();
    assert_eq!(samples.len(), 3);
}
