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

/// Streaming mono resampler to 16 kHz. Keeps the FFT resampler's state
/// between calls, so pieces join without clicks. Memory is bounded by one
/// input chunk plus one output chunk.
pub struct StreamResampler16k {
    inner: Option<Fft<f32>>,
    pending: Vec<f32>,
    to_trim: usize,
    ratio: f64,
    total_in: usize,
    total_out: usize,
}

impl StreamResampler16k {
    pub fn new(in_sample_rate: u32) -> Result<Self, String> {
        const TARGET_RATE: usize = 16000;
        if in_sample_rate == 0 {
            return Err("Sample rate must be greater than 0".to_string());
        }
        let inner = if in_sample_rate as usize == TARGET_RATE {
            None
        } else {
            Some(
                Fft::<f32>::new(
                    in_sample_rate as usize,
                    TARGET_RATE,
                    1024,
                    1,
                    FixedSync::Both,
                )
                .map_err(|e| format!("Failed to create resampler: {:?}", e))?,
            )
        };
        let to_trim = inner.as_ref().map(|r| r.output_delay()).unwrap_or(0);
        Ok(Self {
            inner,
            pending: Vec::new(),
            to_trim,
            ratio: TARGET_RATE as f64 / in_sample_rate as f64,
            total_in: 0,
            total_out: 0,
        })
    }

    /// Runs one chunk (`partial` = valid frames when padding the last one).
    fn run_chunk(&mut self, out: &mut Vec<f32>, partial: Option<usize>) -> Result<(), String> {
        let Some(r) = self.inner.as_mut() else {
            return Ok(());
        };
        let need = r.input_frames_next();
        let take = partial.unwrap_or(need).min(self.pending.len());
        let mut chunk: Vec<f32> = self.pending.drain(..take).collect();
        chunk.resize(need, 0.0);
        let input = InterleavedOwned::<f32>::new_from(chunk, 1, need)
            .map_err(|e| format!("Failed to create input adapter: {:?}", e))?;
        let max_out = r.output_frames_max();
        let mut output = InterleavedOwned::<f32>::new(0.0, 1, max_out);
        let indexing = partial.map(|p| rubato::Indexing::new().partial_len(p));
        let (_, produced) = r
            .process_into_buffer(&input, &mut output, indexing.as_ref())
            .map_err(|e| format!("Resampling error: {:?}", e))?;
        let mut data = output.take_data();
        data.truncate(produced);
        let skip = self.to_trim.min(data.len());
        self.to_trim -= skip;
        out.extend_from_slice(&data[skip..]);
        Ok(())
    }

    /// Feeds mono input; returns the 16 kHz output that is ready now.
    pub fn push(&mut self, input: &[f32]) -> Result<Vec<f32>, String> {
        self.total_in += input.len();
        if self.inner.is_none() {
            self.total_out += input.len();
            return Ok(input.to_vec());
        }
        self.pending.extend_from_slice(input);
        let mut out = Vec::new();
        while self
            .inner
            .as_ref()
            .is_some_and(|r| self.pending.len() >= r.input_frames_next())
        {
            self.run_chunk(&mut out, None)?;
        }
        self.total_out += out.len();
        Ok(out)
    }

    /// Flushes the rest; total output then matches the whole-buffer length.
    pub fn finish(&mut self) -> Result<Vec<f32>, String> {
        let mut out = Vec::new();
        if self.inner.is_none() {
            return Ok(out);
        }
        let expected = (self.ratio * self.total_in as f64).ceil() as usize;
        let left = self.pending.len();
        self.run_chunk(&mut out, Some(left))?;
        let mut guard = 0;
        while self.total_out + out.len() < expected && guard < 64 {
            self.run_chunk(&mut out, Some(0))?;
            guard += 1;
        }
        out.truncate(expected.saturating_sub(self.total_out));
        self.total_out += out.len();
        Ok(out)
    }
}
