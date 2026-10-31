use rubato::audioadapter_buffers::owned::InterleavedOwned;
use rubato::{Fft, FixedSync, Resampler};

/// Downmixes multi-channel interleaved audio to mono f32.
pub fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Result<Vec<f32>, String> {
    if channels == 0 {
        return Err("Channel count must be greater than 0".to_string());
    }
    if channels == 1 {
        return Ok(interleaved.to_vec());
    }

    let channels = channels as usize;
    let frames = interleaved.len() / channels;
    let mut mono = Vec::with_capacity(frames);

    let norm = 1.0 / (channels as f32);
    for frame in interleaved.chunks_exact(channels) {
        let sum: f32 = frame.iter().sum();
        mono.push(sum * norm);
    }

    Ok(mono)
}

/// Resamples a mono f32 buffer from source sample rate to 16 kHz.
pub fn resample_to_16k(input: &[f32], in_sample_rate: u32) -> Result<Vec<f32>, String> {
    const TARGET_RATE: u32 = 16000;

    if in_sample_rate == TARGET_RATE {
        return Ok(input.to_vec());
    }
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let chunk_size = 1024;
    let mut resampler = Fft::<f32>::new(
        in_sample_rate as usize,
        TARGET_RATE as usize,
        chunk_size,
        1,
        FixedSync::Both,
    )
    .map_err(|e| format!("Failed to create resampler: {:?}", e))?;

    let input_owned = InterleavedOwned::<f32>::new_from(input.to_vec(), 1, input.len())
        .map_err(|e| format!("Failed to create input adapter: {:?}", e))?;

    let needed_len = resampler.process_all_needed_output_len(input.len());
    let mut output_owned = InterleavedOwned::<f32>::new(0.0, 1, needed_len);

    let (_consumed, produced) = resampler
        .process_all_into_buffer(&input_owned, &mut output_owned, input.len(), None)
        .map_err(|e| format!("Resampling error: {:?}", e))?;

    let mut result = output_owned.take_data();
    result.truncate(produced);
    Ok(result)
}
