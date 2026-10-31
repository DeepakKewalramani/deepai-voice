//! Runs only when models/ggml-base.en.bin exists; otherwise skips with a message.
//! Set DEEPAI_REQUIRE_MODEL=1 (npm run test:model) to fail instead of skipping.

use std::path::PathBuf;

use deepai_voice_lib::stt::local_whisper::{DEFAULT_MODEL_PATH, LocalWhisper, default_threads};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn load_fixture() -> Vec<f32> {
    let path = repo_root().join("tests/fixtures/audio/sample_en.wav");
    let mut reader = hound::WavReader::open(&path).expect("fixture wav");
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000);
    assert_eq!(spec.channels, 1);
    reader
        .samples::<i16>()
        .map(|s| s.expect("sample") as f32 / 32768.0)
        .collect()
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

#[test]
fn transcribes_fixture_with_base_en() {
    let model = std::env::var("DEEPAI_WHISPER_MODEL")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root().join(DEFAULT_MODEL_PATH));
    if !model.is_file() {
        if std::env::var("DEEPAI_REQUIRE_MODEL").as_deref() == Ok("1") {
            panic!(
                "DEEPAI_REQUIRE_MODEL=1 but model not found at {}",
                model.display()
            );
        }
        eprintln!(
            "SKIP transcribes_fixture_with_base_en: model not found at {}",
            model.display()
        );
        return;
    }
    let mut engine = LocalWhisper::load(&model, default_threads()).expect("load");
    let samples = load_fixture();
    let first = engine.transcribe_timed(&samples).expect("transcribe");
    // Second call proves the loaded model is reused.
    let second = engine.transcribe_timed(&samples).expect("transcribe again");
    eprintln!(
        "TIMING load_ms={} clip_ms={} first_ms={} second_ms={}",
        engine.load_time().as_millis(),
        first.audio.as_millis(),
        first.elapsed.as_millis(),
        second.elapsed.as_millis()
    );
    assert_eq!(
        words(&first.text),
        words("hello world this is a test"),
        "got: {}",
        first.text
    );
}
