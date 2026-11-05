# DeepAI Voice

Mac dictation that types where you are, hands-free: hold **Fn** (or a hotkey), speak, release. The text is typed into whatever app has focus, words appear live as you speak, and corrections are made in place when the final transcript is ready.

Built with Tauri 2 (Rust + React), with local on-device Whisper via whisper.cpp. No audio leaves your Mac.

## Features (v0.1)

- Hold-to-talk dictation into any application
- Live word-by-word preview while you speak
- Automatic corrections when the final transcript arrives
- On-device speech recognition (Whisper, CPU only)
- Dictionary of names and terms, used as hints and corrections
- Floating capsule UI that sticks to the notch / top of the screen

## Install

### Prebuilt (macOS), coming with the v0.1 release

Download `DeepAI-Voice-0.1.0.dmg` (or the `.app` zip) from the [Releases](https://github.com/DeepakKewalramani/deepai-voice/releases) page, open it, and drag the app into Applications. First launch: allow the microphone in System Settings → Privacy & Security → Microphone. macOS will ask for Accessibility permission for global hotkeys and typing into other apps. Both are required for dictation to work.

The current release ships with the English Whisper `base` model. A fine-tuned multilingual model (Hindi, Tamil, Telugu, Bengali, Marathi, Gujarati, Malayalam, Kannada) is ready and will be configurable in an upcoming release; the app already supports pointing the model path via the `DEEPAI_MODEL_PATH` environment variable.

### From source

Requirements: macOS 12+, Node 20+, Rust (stable), and the usual Xcode Command Line Tools (`xcode-select --install`).

```bash
git clone https://github.com/DeepakKewalramani/deepai-voice.git
cd deepai-voice
npm install
npm run tauri dev      # development app
npm run tauri build    # production .app / .dmg under src-tauri/target/release/bundle/
```

Model files are not committed (gitignored). Drop a GGML Whisper model at `models/ggml-base.en.bin` (the default, English) or set `DEEPAI_MODEL_PATH` to any `.bin` model. Downloads: [whisper.cpp models](https://github.com/ggerganov/whisper.cpp?tab=readme-ov-file#whisper).

## Project layout

- `src-tauri/src/`: Rust backend (dictation engine, hotkeys, settings, STT)
- `src/`: React frontend (overlay capsule, app UI)
- `docs/`: research, roadmap, and task notes

## License

MIT, see [LICENSE](LICENSE).
