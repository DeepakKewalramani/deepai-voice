# Independent Code Review: Slices 0.4 and 1.2

- Commits reviewed:
  - Slice 0.4 (Settings + Keychain): `2c99769` and `361852a`
  - Slice 1.2 (Local Whisper STT): `3594f3b` and `0670fc3`
- Reviewer: Antigravity (independent reviewer session)
- Date: 2026-10-31
- Status: APPROVED (Clean, with minor non-blocking notes for future slices)

---

## 1. Slice 0.4: Settings Store + Keychain (`src-tauri/src/settings/`)

### Architecture & Security Assessment
- Conforms strictly to ADR 0002 section 5 (versioned JSON with `schemaVersion`, camelCase keys, and separate Keychain references).
- Zero secret storage: Settings JSON struct explicitly excludes raw key fields; unknown `apiKey` or `key` fields in incoming JSON are cleanly dropped on load and migration.
- Clean credential abstraction: `KeyStore` trait provides `get_key`, `set_key`, `delete_key`, and `has_key`. `KeyringStore` binds to `com.deepakkewalramani.deepaivoice`.
- Dev ergonomics: Environment variable overrides (e.g. `ANTHROPIC_API_KEY`, `DEEPGRAM_API_KEY`) take precedence over Keychain entries.
- Resilient persistence:
  - Missing file safely initializes defaults.
  - Corrupt or invalid JSON triggers timestamped `.bak` backup without crashing.
  - Unrecognized newer schema versions trigger read-only protection to prevent data loss.
  - Granular `take_list` parses items individually so a single corrupt entry does not discard the rest of the configuration.

### Findings & Notes
1. `src-tauri/src/settings/keys.rs` (Non-blocking):
   - Currently uses `keyring = "3.6.3"`. In future updates to `keyring 4.x`, `Entry::new` arguments will change. Current 3.6.3 implementation is stable and works on macOS Keychain.
2. `src-tauri/src/settings/mod.rs` (Non-blocking):
   - In `migrate_v0_to_v1`, `whisper_local` is the sole provider excluded from needing a `keyRef`. As offline models expand (e.g. Ollama localhost or local multilingual whisper), the check should generalize to `!provider.data_leaves_device()`.

---

## 2. Slice 1.2: Local Whisper STT (`src-tauri/src/stt/local_whisper/`)

### Architecture & Performance Assessment
- Clean lifecycle management: Model is loaded once in `LocalWhisper::load()` and state is reused across subsequent transcriptions.
- Privacy compliance: Explicitly calls `whisper_rs::install_logging_hooks()` on initialization to silence whisper.cpp C-level token dumping to stdout/stderr.
- Thread management: Dynamically clamps CPU threads to `[1, 8]` based on available parallelism.
- Warm-up pass: Executes 1 second of silence during load, preventing cold-start latency penalties on first user dictation.
- Error taxonomy: Distinct `WhisperError` variants (`ModelMissing`, `Load`, `Transcribe`) return informative context without leaking transcripts.

### Findings & Notes
1. `src-tauri/src/stt/local_whisper/mod.rs` line 16 (Non-blocking):
   - `DEFAULT_MODEL_PATH: &str = "models/ggml-base.en.bin"` assumes execution from repository root. When running inside bundled `.app` distribution, the path should be resolved via Tauri's `app.path().resource_dir()` or app data directory. This can be wired during slice 2.9 onboarding.
2. `src-tauri/tests/local_whisper_test.rs` line 26 (Observation):
   - Unit test execution on Intel Mac takes ~2.1s for the 1.5s fixture with `base.en`, successfully meeting baseline latency budgets for Phase 1.

---

## Conclusion
Both slices pass all unit tests, linters, clippy (`-D warnings`), formatting checks, and adhere to security rules. Slices 0.4 and 1.2 are approved.

