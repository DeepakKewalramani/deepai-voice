# Research findings: Meeting notes, Live translator, shortcuts, install

Date: 31 Oct 2026. Product: DeepAI Voice. Build target now: Intel Mac only (x86_64, macOS 15.7.4).
Sources: `.agents/tasks/20261031-voice-dictation/research/20` (system audio, virtual mic), `21` (live STT, MT, TTS, consent), `22` (shortcuts, floating bar, install). Each claim there has its source URL. Labels: FACT (seen on official page), INFERENCE, UNKNOWN. Validator results: research/23.

## 1. System audio capture on macOS (no extra driver)
- FACT: Core Audio process taps (`AudioHardwareCreateProcessTap`, `CATapDescription`) need macOS 14.2+ and `NSAudioCaptureUsageDescription`; the user sees a "system audio recording only" prompt, not full Screen Recording. (20)
- FACT: ScreenCaptureKit audio (`capturesAudio`) needs macOS 13+ and Screen Recording permission; `captureMicrophone` on macOS 15+ gives mic plus system audio in one stream. Rust: `screencapturekit` crate (MIT or Apache-2.0). (20)
- UNKNOWN: whether macOS 15.7.4 still shows the monthly Screen Recording re-prompt (only 2024 beta articles found), and whether a process tap avoids it. Spike S8.
- UNKNOWN: whether the `screencapturekit` crate builds on x86_64; cpal loopback states macOS 14.6+ only (INFERENCE from one README line).
- Choice: process tap first, ScreenCaptureKit fallback. Mic and system audio stay as two separate channels, which gives "You" vs "Others" without diarization.

## 2. Virtual microphone (speak into the call), REQUIRED in phase 4
- FACT: no driver-free way found to expose a mic to other apps; private aggregate devices are visible only to the creating process. (20)
- FACT: BlackHole is GPL-3.0; a closed product needs a commercial licence from Existential Audio. INFERENCE: its pkg is signed and notarized (README describes the CI signing script only). (20)
- FACT: Loopback (Rogue Amoeba) is commercial; Background Music is GPL-2.0 and not a fit. (20)
- FACT: Apple's AudioServerPlugIn sample targets macOS 12+, installs into `/Library/Audio/Plug-Ins/HAL` (admin rights INFERENCE) and says to reboot after install. (20)
- Plan: prototype with BlackHole 2ch installed by the user (not bundled). Ship our own AudioServerPlugIn "DeepAI Voice Mic" in a signed, notarized pkg. The user picks "DeepAI Voice Mic" as the microphone in Zoom, Meet or any app.
- UNKNOWN: driver effort, notarization cost, reboot vs `coreaudiod` restart, added latency.

## 3. Live STT across all languages (21)
| Provider | Streaming languages | Auto language ID / code switching | Streaming diarization | Price/hr |
|---|---|---|---|---|
| Deepgram Nova-3 `multi` | 55 for Nova-3 multilingual; `multi` code switching covers 10 (incl. Hindi, no other Indic) | Yes, code switching documented | UNKNOWN | ~0.47 to 0.55 USD |
| Speechmatics Melia 1 | 56 | Yes | Not on Melia (yes on other models) | ~0.40 USD |
| AssemblyAI Universal-3.6 Pro Realtime | 32 | Native code switching | UNKNOWN | see 21 |
| Google Chirp 3 | many | see 21 | None in streaming | see 21 |
| OpenAI `gpt-live-transcribe` | see 21 | see 21 | None | see 21 |
| Sarvam | Indic | Needs `language_code`; `codemix` for Hinglish | UNKNOWN | see 21 |
- No vendor gives Hinglish accuracy or measured latency on our setup (UNKNOWN). Bench before locking defaults.

## 4. Translation (21)
- FACT: Claude Haiku 5.5 is 0.10/0.50 USD per MTok (prompts up to 100k tokens); LLM translation is about 1 to 2 cents per meeting hour (INFERENCE from token estimate). DeepL or Google NMT is about 1.00 to 1.25 USD per hour (prices partly from search summaries).
- FACT: DeepL glossaries (all languages except Thai, 10 MiB, 1,000 per account); Google glossaries need the Advanced edition. LLMs take glossary terms in the prompt.
- Speech-to-speech: OpenAI `gpt-realtime-translate` 0.034 USD/min (FACT, model page); language counts only from secondary sources. Not the default.

## 5. TTS (21)
- FACT: ElevenLabs Flash v2.5, 32 languages, about 100 ms median; a newer model claims 90+ languages. Sarvam Bulbul: English plus 10 Indian languages, no latency figure.
- UNKNOWN: Google and Azure TTS language counts (not researched).

## 6. Recording consent (high level, not legal advice) (21)
- US: federal one-party; all-party states commonly listed as CA, FL, IL, MD, MA, PA, WA (sources disagree on others).
- India: no dedicated call-recording statute found; DPDP Act 2023 applies to personal data (secondary sources).
- EU: GDPR lawful basis plus a notice to participants. China: PIPL treats voice as biometric, separate consent.
- Pattern for us: consent screen on first use, a banner while recording, and a one-click "announce recording" message the user can paste into chat.

## 7. Hold Fn (Globe) and shortcuts (22)
- INFERENCE (Apple page did not load): listen-only `CGEventTap` on `flagsChanged` with the SecondaryFn mask, on its own run-loop thread, gated by Input Monitoring (`CGPreflightListenEventAccess` / `CGRequestListenEventAccess`). Rust: core-graphics 0.25.
- FACT: rdev README says listening needs Accessibility. Which prompt macOS 15.7.4 shows is UNKNOWN; spike S1.
- INFERENCE (third-party sources only): System Settings > Keyboard > "Press fn/Globe key to" (Change Input Source, Emoji, Dictation, Do Nothing). Suppressing it from the app is unverified, so onboarding asks the user to set "Do Nothing".
- FACT: tauri-plugin-global-shortcut gives Pressed/Released for combos; no modifier-only keys is INFERENCE (docs silent). Ctrl+Option+Space stays the no-extra-permission fallback.
- Windows (later): no Fn; `WH_KEYBOARD_LL` with a configurable key (INFERENCE).

## 8. Floating control bar (22)
- FACT: Tauri v2 window config has alwaysOnTop, decorations false, transparent, focusable false, visibleOnAllWorkspaces. macOSPrivateApi is a no-op from Tauri 2.12.1.
- UNKNOWN: drag with focusable false; behaviour over full-screen apps. Fallback tauri-nspanel 2.1.

## 9. Install and update (22)
- FACT: macOS .app/.dmg built on a Mac; free Apple account cannot notarize (99 USD/yr fee is INFERENCE). macOS 15 removed Control-click Open for unsigned apps (INFERENCE, not on Apple page); path is System Settings > Privacy & Security > Open Anyway.
- FACT: Windows MSI (WiX) builds only on Windows; NSIS via cargo-xwin is a last resort. Unsigned Windows apps pass SmartScreen with a warning; Smart App Control blocks them.
- FACT: Tauri updater is off unless the plugin, capability and pubkey are added; updates must be signed; private key in env `TAURI_SIGNING_PRIVATE_KEY`.
- FACT (prior research): TCC grants are keyed to the code signature, so a stable signing identity matters.
