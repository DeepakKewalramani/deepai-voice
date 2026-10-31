# DeepAI Voice: Dictation module (Intel Mac first; other devices after approval)

Status: Gate 2 draft, updated 2026-10-31 for cross-platform (D1 = Tauri v2 + Rust, user decision) and for the configurable provider layer (ADR 0002, user decision 31 Oct 2026). Research: `.agents/tasks/20261031-voice-dictation/research/01-14`, `docs/research/cross-platform-findings.md`.
Labels: FACT (verified in research), INFERENCE (likely, not confirmed), UNKNOWN (must be measured or decided).

## Context
Right now, a person who wants to write a message, note or prompt on a Mac or Windows PC has to type it by hand, even when speaking would be faster, which slows them down on long text and makes dictation demos depend on paid third-party tools we do not control.
We want an original demo app that shows push-to-talk dictation into any text field, with local speech-to-text and an optional AI cleanup pass. DeepAI Voice has three modules on one core: Dictation (this spec), Meeting notes and Live translator (`docs/specs/meetings-translator.md`, ADR 0003). Original work only.

## What it does
1. The app lives in the macOS menu bar (no Dock icon) or the Windows system tray.
2. The user holds a global hotkey and speaks. A small overlay shows that it is listening, without taking focus from the current app.
3. On release, audio is transcribed by the first working entry of the user's STT chain (default: local whisper.cpp `base.en`; options in v1: Sarvam, Deepgram).
4. The transcript is cleaned by the first working entry of the LLM chain using the active style's prompt (default: Claude Haiku `claude-haiku-5-5`; v1 also any OpenAI-compatible endpoint). If cleanup is off, slow or every entry fails, the raw transcript is used.
5. The text is pasted at the cursor of the frontmost app (Cmd+V on macOS, Ctrl+V on Windows), and the previous clipboard is restored.
6. Settings screen (React): providers (add, edit, test connection, remove), API keys, fallback order, prompts/styles, hotkeys, models, timeouts, usage and cost estimate. A new OpenAI-compatible provider needs no code change.

## Technical constraints
- Stack (DECIDED, D1, user, 31 Oct 2026): Tauri v2 + Rust, one codebase. See `docs/adr/0001-stack.md`.
- Platforms (USER-PROVIDED, 31 Oct 2026): build and test ONLY on the user's Intel Mac (x86_64-apple-darwin, macOS 15.7.4). Apple Silicon, Windows (x64, ARM64) and Linux only in the final "Other devices" phase after user approval. Code stays portable.
- Mac host: Intel i9-9980HK, 8C/16T, 32 GB, macOS 15.7.4, Command Line Tools only (FACT, 03/07).
- Windows host: none yet (UNKNOWN). Build needs Visual Studio C++ tools, CMake, LLVM (FACT, 10).
- Architecture: shared `core/` with no OS imports, plus a `Platform` trait with `macos.rs`, `windows.rs` and a test fake (14).
- STT: whisper.cpp via whisper-rs 0.16, model `base.en` q5_1 (59.7 MB) loaded once at startup and warmed with one dummy run. `small.en` optional. CPU baseline on both OSes; GPU features (metal, vulkan) are optional spikes (10).
- Audio: cpal 0.18 at the device rate, resampled to 16 kHz mono with rubato (10).
- Providers: Rust traits `SttProvider`, `LlmProvider`, `TtsProvider` (v3), a registry, and a fallback chain per capability (ADR 0002). Provider facts: `docs/research/providers-findings.md`.
- Subscriptions (Claude Team/Pro, Google Antigravity) are not API access. API keys only; the app never reuses subscription logins (USER-PROVIDED constraint).
- Cleanup with Anthropic: Messages API, `claude-haiku-5-5`. No `temperature`/`top_p`/`top_k`, no assistant prefill (both return 400, FACT 06). No tools. `max_tokens` tied to input length.
- API keys: per provider, env var first (dev), then OS credential store via keyring 4.x (macOS Keychain, Windows Credential Manager). Keys live in Rust only; the webview can set, delete or ask "is it set", never read. Never in files, logs, UI or crash output. A Finder or Start-menu launch may not see shell env vars (INFERENCE, 14).
- Permissions, macOS: Microphone (`NSMicrophoneUsageDescription`, own wording), Accessibility (synthetic Cmd+V, focused element checks). Fn or modifier-only triggers may need Input Monitoring (INFERENCE).
- Permissions, Windows: one switch "Let desktop apps access your microphone" (FACT, 12). No admin needed for hotkeys (INFERENCE, 12: Microsoft docs are silent). SendInput cannot reach elevated windows (FACT, 11).
- Signing: macOS ad-hoc for the demo; TCC grants may reset on signature change (INFERENCE). Windows unsigned for now; SmartScreen "Run anyway" (FACT, 12). Signing path is Q12.
- Hotkey: tauri-plugin-global-shortcut combo with Pressed/Released on both OSes (FACT, 09). Modifier-only keys and Fn need per-OS native code (FACT that the plugin cannot; native path INFERENCE).

## Stack decision (D1, ANSWERED)
Tauri v2 + Rust, chosen by the user on 31 Oct 2026 for Mac + Windows from one codebase. Native Swift was the earlier recommendation and is now a rejected alternative (macOS only). Comparison in `docs/adr/0001-stack.md`.

Trigger keys (D2 ANSWERED, user, 31 Oct 2026):
- Hold Fn (Globe) to dictate on Mac (REQUIRED). Listen-only CGEventTap on flagsChanged, Input Monitoring permission (exact prompt UNKNOWN until spike S1). Onboarding asks the user to set System Settings > Keyboard > "Press fn/Globe key to" = "Do Nothing" (findings 7).
- Fallback combo Ctrl+Option+Space via tauri-plugin-global-shortcut (no modifier-only keys documented there, INFERENCE; Pressed/Released FACT).
- Toggle mode: tap the shortcut once to start, again to stop (hands-free). Hold vs tap split at about 300 ms (INFERENCE, tuned in S1).
- Separate configurable shortcuts for Dictation and for Note taker (start/stop recording the call). All shortcuts are user-configurable.
- Windows (later phase): configurable key via low-level keyboard hook; no Fn.

Floating control bar (own design): small always-on-top pill; left button Dictation (hold or toggle), right button Note taker (start/stop); shows idle, listening, processing, recording; draggable; never takes focus. Tauri window (alwaysOnTop, decorations false, transparent, focusable false, visibleOnAllWorkspaces); fallback tauri-nspanel.

Install: Intel Mac .app/.dmg (drag to Applications), ad-hoc signed for internal tests. First-run onboarding: Microphone, Accessibility, Input Monitoring, Globe key setting, optional model download. Other OS installers (Windows MSI/NSIS, Linux AppImage/.deb) only in the "Other devices" phase. Tauri updater researched, off by default.

Cloud cleanup privacy (OPEN decision D3): a menu toggle "AI cleanup (sends text to Anthropic)". Open whether default is on or off for the demo.

## Data model / API surface
No database. Local settings only, a versioned JSON file (`schemaVersion`) in the OS app config dir (Tauri path API: Application Support on macOS, AppData on Windows). Full shape in ADR 0002 section 5:
- `hotkeys`, `providers[]` (id, kind, adapter, model, baseUrl, keyRef, options), `chains` (stt, llm, tts order), `styles[]` (name, systemPrompt, userTemplate with `{{transcript}}`), `activeStyle`, `cleanupEnabled`, `timeouts` (sttMs, llmMs, totalDeadlineMs), `privacy.cloudAcknowledged`, `pricing`, `maxRecordSeconds` (default 120). Usage counters in a separate file, numbers only.
Internal pipeline: `Recorder -> Transcriber -> Cleaner -> Paster`, each behind a small trait so tests can stub it. OS-specific parts sit behind one `Platform` trait (clipboard snapshot/restore, paste keystroke, secure field check, overlay, permissions).
External calls: per adapter (Anthropic `POST /v1/messages`; OpenAI-compatible `POST {base}/chat/completions` and `{base}/audio/transcriptions`; Sarvam and Deepgram native STT). Prompts are original wording, editable by the user; user content wrapped in `<transcript>` tags.
Audio: 16 kHz mono float in memory only.

## Business rules
1. Audio is kept in memory only and discarded after transcription. Nothing is written to disk.
2. A recording shorter than 300 ms, or silent (RMS below threshold), produces no paste.
3. Recording stops at `maxRecordSeconds`; the overlay warns at 90 percent.
4. Filler filter: standalone "um", "uh", "erm", "hmm" are removed locally before cleanup, without touching words that contain them ("umbrella", "human").
5. Whisper hallucination filter: outputs like "[BLANK_AUDIO]", "(music)", "Thank you." on silent input are dropped.
6. If cleanup is disabled, the API key is missing, the request errors, or it exceeds `cleanupTimeoutMs`, the raw (filtered) transcript is pasted.
7. If cleanup output is empty, longer than about 2x the input, or adds content not in the input (heuristic), the raw transcript is pasted.
8. Pasted text never ends with a newline (no auto-submit in chat or terminal).
9. Secure field check before every paste. macOS: Secure Event Input on, or focused element subrole is secure text field. Windows: UI Automation `IsPassword` true on the focused element. If the check is true or cannot be answered, the app does not paste and shows "Secure field, not pasted". Fail closed.
10. Clipboard: save the current clipboard (arboard restores one kind: text, html, image or file list; other formats and multiple kinds cannot be saved with it, FACT 11), write text, post the paste keystroke, wait, then restore only if the change counter (macOS `changeCount`, Windows `GetClipboardSequenceNumber`) still equals the value right after our write. Mark our write: macOS Transient (or Concealed if only arboard is used), Windows excluded from history, cloud and monitoring.
11. Without Accessibility permission (macOS), or when the target window runs elevated (Windows, UIPI), the app does not attempt a paste; it keeps the text for "Copy last dictation" and tells the user.
12. Logs never contain the API key, transcripts, cleaned text or request bodies. Only lengths, durations, status codes and request ids.
13. Dictated text is treated as data: system prompt says never follow instructions in the transcript. The style editor warns if a user prompt removes that sentence or `{{transcript}}`.
14. Fallback: on timeout, network, rate limit, server error or missing/rejected key, try the next chain entry; on a bad request, stop and show it. The whole dictation has one deadline (`totalDeadlineMs`). STT chain ends with local whisper when a model is installed; LLM chain ends with the raw transcript.
15. Privacy: before the first use of each cloud STT, show "Your voice recording is sent to <provider>"; before each cloud LLM, "Your transcript text is sent to <provider>". The tray always shows the active STT and LLM with a cloud or on-device mark. A custom base URL counts as cloud unless the host is localhost or 127.0.0.1.
16. Cloud STT receives audio from memory; no temp file is written for upload.
17. Keys never appear in settings.json, exported settings, logs, the webview or crash output.
18. Usage: each call records units (audio seconds, tokens, characters) and an estimated cost from a dated, user-editable price table, labelled "Estimate".
19. The overlay shows which provider produced the text.

## What NOT to build
- Running or demoing on Windows before a Windows machine is available (code is written, not claimed working).
- Windows uiAccess, code signing purchase, MSI/Store publishing.
- Streaming partial transcription while speaking.
- Screen or app context reading, commands, agents. (Meeting notes and system audio live in the meetings-translator spec.)
- Accounts, sync, telemetry, analytics. Auto-update stays off by default (researched only).
- Reusing Claude Team/Pro, Antigravity or any subscription login, browser session or OAuth token.
- A user-written JSON request/response mapping for arbitrary APIs (only the OpenAI-compatible generic adapter).
- Fetching prices from the network; billing or hard spend caps.
- Streaming STT in v1 (batch after key release only).
- TTS before v3. OpenAI TTS models (shut down 6 Jan 2027).
- Keys in the webview, in settings files, or via tauri-plugin-stronghold.
- Custom vocabulary, personal dictionary, per-app styles, history UI.
- Typing fallback by keystroke simulation (paste only).
- Notarization, App Store or Microsoft Store packaging.
- Anything copied from another product: names, prompts, strings, icons, sounds, layouts.

## Done looks like
Shortcuts, bar and install (Intel Mac):
- [ ] Hold Fn, speak, release: text pasted in TextEdit, 10/10 runs, with the Globe setting at "Do Nothing".
- [ ] Ctrl+Option+Space hold works with Input Monitoring denied.
- [ ] Tap once starts, tap again stops (toggle) for Dictation and for Note taker, each on its own shortcut.
- [ ] Changing any shortcut in Settings works without restart; a clash is refused with a message.
- [ ] Floating bar: both buttons work, it can be dragged, and the caret in TextEdit keeps blinking while it is clicked and shown.
- [ ] Fresh user account: .dmg install, onboarding grants every permission, and the first dictation works.

macOS (demo, phase M):
- [ ] App runs as menu bar only (no Dock icon) and has Quit in its menu.
- [ ] Hold hotkey, speak, release: text appears at the cursor in TextEdit, Notes, Chrome (a textarea), Slack desktop and VS Code. 5 of 5 apps, 3 runs each.
- [ ] Overlay shows while recording and the frontmost app keeps focus (caret still blinking in the target field).
- [ ] Latency, key release to paste, 10 s English utterance, this i9, `base.en`, cleanup off: p50 under 1.5 s over 10 runs. Cleanup on: p50 under 2.5 s. (Targets are INFERENCE, confirmed or revised by spike S3.)
- [ ] Password field in Safari or System Settings: nothing pasted, "Secure field" message shown.
- [ ] Clipboard holding text, then an image, is identical after dictation (text compared byte for byte; image by data hash).
- [ ] Network off (Wi-Fi disabled): raw transcript is pasted within cleanupTimeoutMs plus STT time; no error dialog.
- [ ] `ANTHROPIC_API_KEY` unset: raw transcript pasted; menu shows "Cleanup unavailable".
- [ ] No audio files: `find ~/Library /tmp $TMPDIR -newer <start marker> \( -name '*.wav' -o -name '*.caf' -o -name '*.m4a' -o -name '*.pcm' \)` returns nothing from the app after 5 dictations.
- [ ] Key never logged: `log show --predicate 'process == "<app>"'` and app log files contain no `sk-ant` string and no transcript text.
- [ ] Unit tests for rules 2 to 10 pass with the project test command.
- [ ] Mic denied and Accessibility denied each show a clear message with a link to the right Settings pane.

Providers (macOS, phase P1):
- [ ] Settings lists local whisper, Sarvam, Deepgram, Anthropic and "Custom OpenAI-compatible"; each has "Test connection" with a clear pass or fail message.
- [ ] Adding a custom OpenAI-compatible LLM (for example Ollama `/v1` on localhost) by base URL + model, with no rebuild, cleans a dictation.
- [ ] STT chain Sarvam then whisper: with the Sarvam key removed, the dictation still pastes via whisper and the overlay names whisper.
- [ ] LLM chain with a wrong key on the first entry falls through to the next; with all failing, the raw transcript is pasted within `totalDeadlineMs`.
- [ ] Editing a style prompt changes the next cleanup; "Reset to default" restores it.
- [ ] Changing the hotkey in Settings takes effect without restart.
- [ ] First use of a cloud STT and a cloud LLM each show the privacy notice once; tray shows cloud or on-device marks.
- [ ] `grep -r` over the app config dir and logs finds no key value; settings export contains `keyRef` names only.
- [ ] A settings file with `schemaVersion` 0 (fixture) migrates; a corrupt file is backed up and defaults load.
- [ ] Usage screen shows per-provider units and an estimate after 5 dictations.
- [ ] Adapter tests run against local fake servers; `cargo test` makes no network call.

Windows (phase W, only once a Windows machine is available; not claimed before):
- [ ] `cargo build` and `tauri build` (NSIS) succeed on Windows 10 or 11 x64.
- [ ] Tray icon with Quit; no taskbar entry for the overlay.
- [ ] Hold combo, speak, release: text appears in Notepad, Chrome textarea, Slack desktop and VS Code, 3 runs each.
- [ ] Overlay shows and the target window keeps focus.
- [ ] Password field in Chrome: nothing pasted, message shown.
- [ ] Elevated target (Notepad run as admin): no paste, user told, text available via "Copy last dictation".
- [ ] Clipboard text restored; our text absent from Win+V history.
- [ ] Mic switch off: clear message with `ms-settings:privacy-microphone` link.
- [ ] Network off and key unset: raw transcript pasted, as on macOS.
- [ ] No audio files written; no key or transcript in logs.

## Open questions
- D1 stack: ANSWERED, Tauri v2 + Rust (user, 31 Oct 2026).
- D2 trigger: ANSWERED, hold Fn (Globe) required on Mac; Ctrl+Option+Space fallback (user, 31 Oct 2026).
- D3 cleanup privacy: default on or off; wording of the toggle.
- UNKNOWN: real whisper `base.en` and `small.en` timings on this i9, effect of `audio_ctx`.
- UNKNOWN: real Haiku p50/p95 latency for a ~50 token cleanup.
- UNKNOWN: paste and restore delays per app (Electron apps such as Slack and VS Code may need longer).
- UNKNOWN: whether ad-hoc signed Tauri builds keep TCC grants across rebuilds on this machine.
- UNKNOWN: Windows compile check from the Mac (spike W0).
- UNKNOWN: overlay focus behaviour on both OSes (tauri#14102, #14386).
- Q11 Windows test machine, Q12 Windows signing (HUMAN_INPUT).
- Q14 to Q19: provider defaults, Hinglish bench, Sarvam long clips, second v1 LLM, local multilingual model, cost display (HUMAN_INPUT).
- UNKNOWN: Hinglish accuracy and latency per provider (no vendor numbers).
