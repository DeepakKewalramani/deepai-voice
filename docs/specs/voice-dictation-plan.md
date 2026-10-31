# Plan: Tray / menu bar voice dictation (macOS first, Windows later)

Spec: `docs/specs/voice-dictation.md`. ADRs: `docs/adr/0001-stack.md`, `docs/adr/0002-provider-layer.md` (Tauri v2 + Rust, D1 answered 31 Oct 2026).
Status: draft, waiting for D2 (trigger), D3 (cleanup default), Q11 (Windows machine), Q12 (Windows signing) and Gate 1-3 approval.
No step starts before Gates 1 to 3 are approved. No installs without approval. Each step ends with its own check.
Layout (research/14): `src-tauri/src/{main,controller}.rs`, `core/*` (no OS imports), `platform/{mod,macos,windows,fake}.rs`, small web UI for overlay and settings, git-ignored `models/`.
Test command: `cargo test` (core and fake platform run on any OS).

## Phase M0: Mac spikes (throwaway code in `spikes/`)

S0. Build and sign pipeline
- Check Rust is 1.90 or newer (plugin MSRV, FACT 09). `tauri build --bundles app`, ad-hoc signed (`signingIdentity "-"`), copied to a fixed path, Info.plist with `NSMicrophoneUsageDescription` and Accessory policy.
- Check: rebuild twice; Microphone and Accessibility grants survive both rebuilds (no re-prompt). Record result; if grants reset, document the demo setup step.

S1. Trigger capture
- tauri-plugin-global-shortcut with the D2 combo, log Pressed and Released.
- Check: 20/20 press and release pairs logged with timestamps. Extra cases from the flow check: quick tap under 300 ms, holding while another app opens, pressing again while busy (ignored). Fn / Right Option only if D2 asks: native flagsChanged spike, record permission needed and Globe popup.

S2. Non-focus overlay
- Tauri window: `focusable(false)`, always on top, `set_ignore_cursor_events(true)`, transparent. Fallback: tauri-nspanel.
- Check: with TextEdit focused, show and hide 10 times; caret keeps blinking and typed keys still reach TextEdit. Also click the overlay area (tauri#14102).

S3. Whisper speed on this Intel
- whisper-rs 0.16, CPU, `base.en` q5_1 and `small.en` q5_1 on fixed 5 s, 10 s and 30 s clips, default and reduced `audio_ctx`, 8 threads (approval needed: Q10). Optional: `metal` feature run.
- Check: table of wall times (5 runs each) and word errors on the 10 s clip. Pass if `base.en` 10 s is under 1.0 s; else revise spec targets.

S4. Paste matrix
- Own `paste()` (CGEvent Cmd+V) plus arboard text save/restore with changeCount guard.
- Check: TextEdit, Notes, Chrome, Slack, VS Code, Terminal x delays 50/150/300 ms; smallest delay with correct paste and restore in 3/3 runs. Password field: Secure Event Input and AX subrole both detected. Also check whether the app needs a restart after Accessibility is granted (UNKNOWN, 13).

S5. Haiku latency
- 20 calls with a 10 s style transcript (approval needed: Q9). Check: p50/p95 recorded; sets `cleanupTimeoutMs`.

S6. Keychain with keyring 4.x
- Store and read a dummy (not real) key from an ad-hoc signed build, rebuild, read again. Check: whether macOS prompts after re-sign; record it.

S7. Hinglish bench (approval needed: Q15, paid)
- 10 own recordings (English, Hindi, Hinglish, 5 to 30 s). Sarvam `saaras:v3` codemix (and v4 to test whether codemix works), Deepgram `nova-3` (`hi` and `multi`), local whisper `small` multilingual; cleanup with Haiku and one more LLM. Check: table of word errors (manual) and latency per provider; picks defaults for Q14.

W0. Windows compile check from the Mac (no Windows machine needed)
- Try in order: `cargo check --target x86_64-pc-windows-msvc` with a `whisper` Cargo feature turned off; cargo-xwin; a GitHub Actions `windows-latest` job (needs a repo, so only with approval). All INFERENCE until tried.
- Check: `platform/windows.rs` and all `cfg(windows)` code type-check. If none works, record it and rely on phase W.

Gate: spike results in CHECKPOINT.md; user approves D2, D3 and any spec change.

## Phase M1: Core logic, test first (shared, no OS code)
Each business rule gets a failing test first, then the smallest code to pass.
1. Filler filter (rule 4). Tests: standalone fillers removed with punctuation; "umbrella", "human", "Uhura" kept; empty stays empty.
2. Hallucination and short-clip gate (rules 2, 5). Tests: under 300 ms gives none; silent buffer gives none; "[BLANK_AUDIO]" and lone "Thank you." on low RMS dropped.
3. Cleaner fallback (rules 6, 7, 13). Stub HTTP. Tests: disabled, missing key, HTTP 500, timeout, empty output, output over 2x input each return raw text. Request has no temperature and no prefill; transcript wrapped in `<transcript>`.
4. Finalizer (rule 8). Tests: trailing newlines and spaces trimmed.
5. Paste orchestration with FakePlatform (rules 9, 10, 11). Tests: secure field true or unknown gives refusal; restore skipped when the change counter moved; marking requested on our write; no Accessibility or elevated target gives no paste and text kept for "Copy last dictation".
6. Logging (rule 12). Tests: `sk-ant` redacted; pipeline lines contain lengths only.
7. Settings and key provider. Tests: defaults (`maxRecordSeconds` 120, `cleanupTimeoutMs`), env var wins over keyring stub, missing key gives "Cleanup unavailable".
Check for phase: `cargo test` green, test names listed in the checkpoint.

## Phase M2: Shared adapters (same code both OSes, run on Mac)
8. Recorder: cpal at device rate, match on SampleFormat, rubato to 16 kHz mono, 50 ms start discard, max length cap (rules 1, 3). Check: 10 s gives 160000 samples +- 1 percent; no file created (spec find command).
9. Transcriber: whisper-rs, model loaded at startup, warm-up run. Check: transcript of fixed clip matches S3.
10. Key provider: env var, then keyring (Keychain on Mac). Check: key absent from logs.

## Phase M3: macOS platform and app shell
11. `platform/macos.rs`: CGEvent paste, NSPasteboard marking and changeCount, secure field (Secure Event Input + AX subrole), AXIsProcessTrusted check before every paste, mic status. Check: S4 matrix re-run through the real code path.
12. Tray menu: status, cleanup toggle (D3), model choice, "Copy last dictation" (memory only, cleared on quit), Quit. Accessory policy. Check: no Dock icon; Quit exits.
13. Hotkey and overlay wired with level meter; busy state ignores presses. Check: hold, speak, release works in TextEdit end to end.
14. Permission flow: mic and Accessibility status, explainer, Settings deep link, recheck on activate. Check: `tccutil reset` both, first-launch flow grants both; record whether a restart was needed.
15. Error handling for E1, E9 and E15 (flow doc section 11). Check: each error path shows its message.

## Phase M4: Verify Mac against spec
16. Run every macOS "Done looks like" item; record commands and results in the checkpoint.
17. `code-review` skill on the full diff; security pass on key handling and clipboard.
18. Demo rehearsal on this Mac with network on and off.

## Phase W: Windows (later, needs a Windows machine, Q11)
Code for W1 to W3 may be written during M3 behind `cfg(windows)` and type-checked by W0, but nothing is claimed working until W4.
W1. `platform/windows.rs`: SendInput Ctrl+V with return check; arboard write with history, cloud and monitoring exclusion; `GetClipboardSequenceNumber` guard; UIA `IsPassword` on the focused element (unknown blocks); target-window elevation check (skip paste under UIPI). Check: unit tests on FakePlatform unchanged and green; `cargo check` for the Windows target (W0).
W2. Windows shell: tray, overlay with `focusable(false)` (tauri#14386), mic denial detection by silence or open failure, `ms-settings:privacy-microphone` link. Check: on the Windows machine, overlay does not steal focus in Notepad.
W3. Windows build: MSVC, CMake, LLVM (`LIBCLANG_PATH`), `GGML_NATIVE=OFF` with AVX2/FMA/F16C, NSIS bundle, WebView2 downloadBootstrapper. Check: installer runs on a clean user; SmartScreen behaviour recorded; Defender scan clean.
W4. Run every Windows "Done looks like" item, including the hotkey Released timing (polling, 09) and paste delays per app. Record results.
W5. `code-review` skill on the Windows diff.

## Provider phases (ADR 0002)
Code lives in `src-tauri/src/providers/{mod,registry,chain,usage}.rs` plus one file per adapter. Every adapter is tested against a local fake HTTP server; no paid call in `cargo test`.

### Phase P1 (v1, macOS, after M3)
P1.1 Traits, `ProviderError` kinds, `Usage`, registry from settings. Tests: unknown adapter rejected; duplicate ids rejected.
P1.2 Fallback chain and total deadline (rule 14). Tests with fakes: timeout, 401, 429, 500 move on; 400 stops; deadline returns raw text.
P1.3 Settings v1 with `schemaVersion`, migrations, corrupt-file backup. Tests: fixture migration, unknown newer version opens read-only.
P1.4 Key store commands `set_key`, `delete_key`, `has_key` (keyring 4.x, S6 result). Commands restricted per window. Check: no key in config dir, logs or export (spec grep).
P1.5 STT adapters: `whisper_local` (move existing), `sarvam_stt` (REST under 30 s; longer clips per Q16), `deepgram_stt` (pre-recorded). Check: fake-server tests; one real call each only with approval.
P1.6 LLM adapters: `anthropic` (no temperature, no prefill, effort low) and `openai_compatible` (standard fields only, `n=1`). Second named LLM preset per Q17. Check: fake-server tests; Ollama `/v1` on localhost if installed (approval for install).
P1.7 Styles: built-in read-only plus duplicate, edit, reset; `{{transcript}}` check and data-safety warning (rule 13).
P1.8 Privacy notices and tray marks (rule 15). Usage counters and estimate (rule 18) with dated price defaults from findings.
P1.9 Settings UI (React): providers with Test connection, keys, chains (drag order), styles, hotkeys, timeouts, usage. Check: every "Providers" Done item in the spec.
P1.10 `code-review` skill on the provider diff; security pass on key handling.

### Phase P2 (v2)
- More presets on the generic adapter: OpenAI, Gemini compat, OpenRouter, Groq, Together, Ollama. Native or compat adapters for ElevenLabs Scribe, AssemblyAI, OpenAI `gpt-transcribe`, Groq Whisper. Google STT only if service-account auth is accepted.
- Per-style hotkeys; streaming STT (Sarvam or Deepgram WebSocket) behind a setting.
- Check: each preset passes Test connection and one dictation (paid calls with approval).

### Phase P3 (v3, TTS)
- `TtsProvider`: macOS built-in (Lekha for Hindi) first, then Sarvam Bulbul and ElevenLabs. Not OpenAI TTS (shut down 6 Jan 2027). Windows WinRT later.
- Use case to be defined before P3 starts (read-back of selection or replies). Check: spoken output for an English and a Hindi line.

## Risks
- TCC grants lost on rebuild (S0).
- Overlay steals focus (S2, W2; fallback tauri-nspanel on Mac).
- whisper slower than target on this CPU (S3; fallback `tiny.en`).
- Electron apps need longer paste delay (S4, W4).
- Windows parts untested until a machine exists (W0 reduces compile risk only).
- Unsigned Windows builds blocked by Smart App Control or flagged by antivirus (Q12).
- Provider APIs, model ids and prices change often (OpenAI STT and TTS already have shutdown dates). Mitigation: models are settings, not code; price table dated and editable.
- Hinglish quality unknown until S7.
- Sarvam REST under 30 s vs 120 s recordings (Q16).

## Additions from the flow check (31 Oct 2026, DEFAULT pending user answer)
Source: docs/research/voice-dictation-flow.md section 11. Folded into S1, steps 5, 12, 13, 14, 15 above.
- Extra S1 test cases and a busy-state test (hotkey ignored while processing). DEFAULT (Q6 A).
- "Copy last dictation" menu item (memory only, cleared on quit). DEFAULT (Q5 B).
- Check whether the app must restart after Accessibility is granted; check trust before every paste. DEFAULT.
- Error handling for E1, E9 and E15. DEFAULT.
- `maxRecordSeconds` = 120. DEFAULT (Q4 B).

## Meeting notes and Live translator (added 31 Oct 2026; spec `docs/specs/meetings-translator.md`, ADR 0003)
Phase order follows `docs/ROADMAP.md`: 1 Foundation, 2 Dictation, 3 Meeting notes, 4 Live translator, 5 Advanced, 6 Other devices (approval only). Intel Mac only.

New spikes (throwaway code in `spikes/`, before phase 3):
S8. System audio on Intel macOS 15.7.4: Core Audio process tap via objc2, then the `screencapturekit` crate. Check: 30 s of YouTube audio captured as a separate channel; the permission prompt is recorded; whether a re-prompt appears after a restart.
S9. Streaming STT latency and language ID (paid, approval Q15/Q22): Deepgram Nova-3 `multi` (code switching covers 10 languages incl. Hindi; other Indic languages need an explicit code), Speechmatics, Sarvam `codemix`. Check: delay table and manual word errors on English, Hindi, Hinglish and Chinese clips.
S10. SQLite + FTS5 + encryption option (SQLCipher vs Keychain key). Check: 10k segments insert and search under 50 ms; encrypted file unreadable without the key.
S11. Virtual mic: user-installed BlackHole 2ch (approval needed, Q21), play TTS into it, and select it in a test Meet call. Check: the other side hears it; latency measured. Driver build plan for "DeepAI Voice Mic" after Q24.
S12. Fn key: CGEventTap flagsChanged on this Mac. Check: which permission prompt appears; 20/20 hold events; Globe setting effect.

Risks added: Screen Recording re-prompt; driver signing needs a paid Apple account; streaming diarization weak (mitigated by two channels); Hinglish and Chinese quality unknown until S9; consent law varies by country (reminder only, not legal advice).
