# Test Audio Fixtures Plan

This directory contains standardized audio fixtures used for automated testing of the DeepAI Voice audio capture, resampling, silence detection, Whisper local STT, cloud STT adapters, and filler filters.

## Audio Specification
- Format: RIFF WAVE (`.wav`)
- Encoding: Linear PCM 16-bit signed little-endian (`LEI16`)
- Channels: 1 (Mono)
- Sample Rate: 16,000 Hz (16 kHz standard for Whisper and modern STT engines)
- Generation Tool: macOS built-in `say` (voice: Samantha) + `afconvert` utility, and Python `wave` module for digital silence.

## Fixture Manifest
| Fixture File | Spoken Text | Pipeline Test Purpose |
|---|---|---|
| `01_english_greeting.wav` | "Hello world, this is a test of voice dictation." | Baseline Whisper local STT transcription accuracy and latency measurement. |
| `02_english_email.wav` | "Please review the pull request and let me know your thoughts." | Business and productivity dictation evaluation. |
| `03_filler_words.wav` | "Um, I was thinking, uh, we should ship this today, you know." | Filler word detection and stripping filter (Slice 1.3). |
| `04_technical_terms.wav` | "DeepAI Voice runs on Tauri version two with Rust and React." | Technical vocabulary transcription and case formatting. |
| `05_numbers_and_punctuation.wav` | "Meeting at three PM with twelve engineers in room four." | Numbers, times, and quantitative phrasing. |
| `06_silence.wav` | (None, 1.0 second silence) | Silence gate verification (RMS < threshold must return None without transcribing). |

## How to Regenerate
Run from repo root:
```bash
say -v Samantha -o /tmp/raw1.aiff "Hello world, this is a test of voice dictation." && afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/raw1.aiff tests/fixtures/audio/01_english_greeting.wav
say -v Samantha -o /tmp/raw2.aiff "Please review the pull request and let me know your thoughts." && afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/raw2.aiff tests/fixtures/audio/02_english_email.wav
say -v Samantha -o /tmp/raw3.aiff "Um, I was thinking, uh, we should ship this today, you know." && afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/raw3.aiff tests/fixtures/audio/03_filler_words.wav
say -v Samantha -o /tmp/raw4.aiff "DeepAI Voice runs on Tauri version two with Rust and React." && afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/raw4.aiff tests/fixtures/audio/04_technical_terms.wav
say -v Samantha -o /tmp/raw5.aiff "Meeting at three PM with twelve engineers in room four." && afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/raw5.aiff tests/fixtures/audio/05_numbers_and_punctuation.wav
python3 -c "import wave; f=wave.open('tests/fixtures/audio/06_silence.wav', 'w'); f.setnchannels(1); f.setsampwidth(2); f.setframerate(16000); f.writeframes(b'\x00\x00'*16000); f.close()"
rm -f /tmp/raw*.aiff
```

