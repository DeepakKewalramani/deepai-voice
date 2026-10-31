# ADR 0003: Meeting notes and Live translator on one shared core

- Status: Proposed (31 Oct 2026). Gates 1 to 3 approved by user on 31 Oct 2026; this ADR records the design for those gates.
- Sources: `docs/research/meetings-translator-findings.md`, research/20 to 23, ADR 0001 (stack), ADR 0002 (provider layer).

## Context
USER-PROVIDED (31 Oct 2026): DeepAI Voice is our own product with three modules on one core: Dictation, Meeting notes and Live translator. All languages in and out, chosen in Settings. Meeting capture works with any call app (Zoom, Meet, Teams, WeChat, browser) through one toggle, "Record this call", with no bots and no per-app integrations. The translator shows live subtitles, suggests replies, translates the user's own reply, and either pastes it to chat or speaks it into the call. Speaking into the call is required in phase 4. Build and test only on the user's Intel Mac until other targets are approved.

## Decision

### 1. Shared audio engine (Rust)
- Two independent channels: `mic` (cpal) and `system` (Core Audio process tap, macOS 14.2+; ScreenCaptureKit audio as fallback). Channel = speaker side: mic is "You", system is "Others". No diarization needed for that split; provider diarization, where it exists, only splits "Others" further.
- Each channel is resampled to 16 kHz mono and published as frames on an in-process bus. Modules subscribe; they never open devices themselves.
- Dictation uses `mic` only, as in ADR 0001. Meeting notes and Translator use both.
- Output side: an `AudioSink` trait with `Speakers` and `VirtualMic`. `VirtualMic` writes to "DeepAI Voice Mic" (own AudioServerPlugIn, pkg installed into /Library/Audio/Plug-Ins/HAL; admin rights INFERENCE, reboot per Apple sample). During the prototype it writes to BlackHole 2ch if the user installed it themselves. We do not bundle BlackHole (GPL-3.0).

### 2. Pipelines per module
- Dictation: unchanged (record, STT batch, cleanup, paste).
- Meeting notes: frames, then streaming STT per channel (`SttProvider::transcribe_stream`, added from ADR 0002 "later"), then segments into the session store. At stop: summary, decisions and action items through the LLM chain, then search index update.
- Live translator: the Meeting notes pipeline plus a translate step per final segment (LLM chain by default, DeepL when a glossary is set) into the user's chosen languages, with the original kept. Optional "simple wording" pass. Reply flow: (a) suggested replies from the whole conversation so far, (b) the user types or speaks a reply in any language, then translate. Output goes to paste-to-chat or to TTS, then `VirtualMic` and optionally `Speakers`.
- Glossary: user list of term pairs, injected into LLM prompts, mapped to DeepL glossaries where used. The same term is always translated the same way.

### 3. Session store
- Local SQLite in the app data dir: `sessions`, `segments` (channel, start/end ms, original text, detected language, translations JSON, provider), `summaries`, `glossary`, FTS5 index over original and translated text.
- Audio is not stored by default (setting "Keep audio", off). Encryption at rest is an option (SQLCipher or a Keychain-held key; choice in spike S10).
- Export: Markdown first; Google Docs later.

### 4. Controls
- Shortcuts, all configurable: Dictation hold (Fn on Mac, fallback Ctrl+Option+Space), Dictation toggle, Note taker start/stop. Fn via a listen-only CGEventTap (Input Monitoring); combos via tauri-plugin-global-shortcut.
- Floating control bar: our own small always-on-top pill. Left button Dictation, right button Note taker; it shows state, can be dragged and does not take focus. A plain Tauri window comes first, with tauri-nspanel as the fallback.

### 5. Consent and privacy
- First use: consent screen. While recording: a visible banner and a tray state. One click copies an "I am recording this call with DeepAI Voice" message for the user to post. Privacy notices from ADR 0002 rule 15 also cover system audio and translation text.

### 6. Install
- Tauri bundler. Now: .app/.dmg for Intel Mac with ad-hoc signing for internal testing. Later, after approval: Developer ID plus notarization (also needed for the virtual mic pkg), Windows NSIS/MSI, Linux AppImage/.deb. The updater is researched and off by default.

## Alternatives considered
- Per-app bots or SDKs (Zoom, Meet): rejected by the user; they are per app and need accounts.
- ScreenCaptureKit only: it needs full Screen Recording permission and may re-prompt; we keep it as the fallback.
- Bundle BlackHole: GPL-3.0 needs a commercial licence for a closed product. User-installed BlackHole is for the prototype only.
- Rogue Amoeba Loopback: commercial, and each user must buy it.
- Speech-to-speech models (OpenAI realtime translate): they lose the separate text transcript step and cost more per hour. Kept as an optional provider later.
- Diarization only (one mixed stream): weaker in streaming (findings section 3); the two-channel design gives You and Others for free.

## Consequences
- The virtual mic means a signed and notarized driver pkg, which needs a paid Apple Developer account (open question Q24). Until then the prototype relies on BlackHole installed by the user.
- Streaming STT becomes a v1 need for meetings, earlier than ADR 0002 planned.
- New permissions on Mac: system audio capture, Input Monitoring (Fn), and likely admin for the driver install (INFERENCE).
- Sessions on disk change rule 1 of the dictation spec (audio in memory only). Dictation keeps that rule. Meetings store text and, only with the opt-in setting, audio.
