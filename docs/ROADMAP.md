# DeepAI Voice: phase-wise roadmap

Status: DRAFT for user approval (31 Oct 2026, updated for Meeting notes, Live translator, shortcuts and install). Gates 1 to 3 approved by user 31 Oct 2026. Details per slice live in `docs/specs/` and are filled by the planning jobs.
Stack: Tauri v2 + Rust (native parts) + React + TypeScript + Vite. Build target now: the user's Intel Mac only (x86_64, macOS 15.7.4). Other devices only after user approval. Provider layer (any STT, LLM, TTS provider, custom providers).
First demo target: Dictation (full) + Live translator preview (USER-PROVIDED, 31 Oct 2026).

## How we work (applies to every phase)
- One branch per phase: `phase-<n>-<name>`, cut from `main`. Each slice is one small PR into the phase branch (or a stacked PR), one area each.
- Every slice is small (target under ~300 lines), working and testable on its own. No slice is "done" until its check passes on this Mac.
- Each slice has: goal, files touched, test (automated where possible), manual check, "Done looks like".
- Before each PR: tests, lint, typecheck, build, `code-review` skill, secret check. Clean squashed commits. Push and merge only with `APPROVE PUSH <branch>` / `APPROVE MERGE <target>`.
- A phase ends with a demo check on the real app and a merge into `main` after approval.

## Phase 0: Repo and foundation setup (branch `phase-0-setup`)
| Slice | What | Check |
|---|---|---|
| 0.1 | git init, .gitignore (models, .env, .agents), README, docs map | `git status` clean, ignored files not tracked |
| 0.2 | Tauri v2 + React + TS + Vite scaffold, menu bar app, no Dock icon | App launches from `npm run tauri dev`, icon in menu bar |
| 0.3 | Lint, format, typecheck, Rust fmt/clippy, unit test runners, CI-ready scripts | All commands pass on empty app |
| 0.4 | Settings store (versioned JSON) + Keychain key storage | Unit tests: save, load, migrate; key never in file |
| 0.5 | Logging with redaction (no keys, no text) | Test: log output contains no key or transcript |

## Phase 1: Foundation: audio, local STT, provider core (branch `phase-1-foundation`)
| Slice | What | Check |
|---|---|---|
| 1.1 | Mic permission flow + capture 16 kHz mono in memory | Record 5 s, level meter moves, nothing written to disk |
| 1.2 | Whisper `base.en` loaded once, transcribe a WAV fixture | Test fixture returns expected text; time measured and logged |
| 1.3 | Silence, too-short and hallucination filters | Unit tests on fixtures (silence gives no text) |
| 1.4 | Provider traits (STT, LLM, TTS), registry, fallback chain + local Whisper adapter | Unit test: first fails, second answers |
| 1.5 | Audio engine bus: channels publish frames, modules subscribe | Test: two fake channels, two subscribers, no frame lost |

## Phase 2: Dictation (branch `phase-2-dictation`), DEMO PART 1
| Slice | What | Check |
|---|---|---|
| 2.1 | Hold Fn (CGEventTap, Input Monitoring) + Ctrl+Option+Space fallback | 20/20 press and release pairs logged for both |
| 2.2 | Toggle mode + separate configurable shortcuts (Dictation, Note taker) | Tap starts, tap stops; rebinding works without restart |
| 2.3 | Floating control bar (pill, two buttons, state, drag, no focus) | Caret in TextEdit keeps blinking while bar is used |
| 2.4 | Paste at cursor + clipboard restore + Accessibility check | Text appears in TextEdit, Notes, Slack, browser; old clipboard back |
| 2.5 | Secure field block | Nothing pasted in a password field |
| 2.6 | Cleanup via LLM chain (Anthropic + custom OpenAI-compatible) + filler filter fallback | Mocked API tests; offline still pastes raw text |
| 2.7 | Cloud STT adapters (Deepgram, Sarvam), one per slice | Fake-server test per adapter, real call with user key and approval |
| 2.8 | Settings UI: providers, keys, languages, prompts, shortcuts; usage estimate; privacy notices | UI test + manual check, keys masked |
| 2.9 | Onboarding (permissions, Globe key setting, model download) + .dmg build | Fresh macOS user: install, onboard, dictate |
| 2.10 | Phase demo check | 10 dictations in 4 apps, timings recorded |
| 2.13 | Provider hub: catalog of best providers per capability (STT, LLM, TTS), one-tap connect with masked key entry, connected status | Add a provider from the catalog in Settings; key never in settings.json; entry appears in the chain editor |
| 2.14 | BYOK custom provider "Make Your Own": endpoint URL, API key, model, protocol (OpenAI-compatible default); Test Connection | Custom endpoint answers a test ping; provider is usable like any built-in |

## Phase 3: Call audio + Live translator (branch `phase-3-translator`), DEMO PART 2
| Slice | What | Check |
|---|---|---|
| 3.1 | System audio capture (process tap; ScreenCaptureKit fallback) as "Others" channel | Audio from Zoom, Meet in Chrome and YouTube captured, mic separate |
| 3.2 | Streaming STT with auto language detection, per channel | Live text for both channels; delay measured |
| 3.3 | "Record this call" toggle + consent screen + recording banner | Shown on first use; banner visible while capturing |
| 3.4 | Translate each segment to chosen languages, original kept, simple-wording option | Mocked LLM tests; manual check with Hindi and Chinese clips |
| 3.5 | Subtitles window | Readable live subtitles during a test call |
| 3.6 | Glossary (LLM prompt + DeepL glossary when used) | 5 terms translated identically across 3 sessions |
| 3.7 | Suggested replies from the whole conversation (both languages) | 2 to 3 replies shown; nothing sent automatically |
| 3.8 | Own reply typed or spoken in any language, translated | Hindi reply becomes English text |
| 3.9 | Send: paste to chat | Reply lands in the call app chat box |
| 3.10 | TTS adapter (ElevenLabs default, provider chain) + play locally | Voice plays in the target language |
| 3.11 | Speak into the call: virtual mic (BlackHole user-installed for prototype; own "DeepAI Voice Mic" driver after Q24) | The other side hears it in a test call; install logged in INSTALLED.md |
| 3.12 | Phase demo check | Full 3-minute demo script passes twice |

## Phase 4: Meeting notes (branch `phase-4-meetings`)
| Slice | What | Check |
|---|---|---|
| 4.1 | Session store (local SQLite, FTS5), encrypted-at-rest option | Session saved, reloaded after restart, search works |
| 4.2 | Summary, decisions, action items | Output checked on 3 sample meetings |
| 4.3 | Export Markdown | Export has every segment with side, time and text |
| 4.4 | Bilingual transcript saved, nothing lost offline | Network-off test: segments kept as pending |
| 4.5 | Phase demo check | 15-minute test call recorded end to end twice |

## Phase 5: Advanced (branch `phase-5-advanced`), Intel Mac only
Polish and more providers: Google Docs export, more STT/MT/TTS presets, personal word list, spoken corrections, signed and notarized build (Q24), updater (off by default), performance tuning.

## Phase 6: Other devices (branch `phase-6-other-devices`), ONLY after user approval
Apple Silicon Mac (aarch64 or universal), Windows (x64 and ARM64: WASAPI loopback, virtual mic, configurable dictation key, MSI/NSIS), Linux (PipeWire monitor and virtual source, AppImage/.deb). These need test machines (Q11). Code stays portable before this phase, but nothing is built or tested for these targets.

## Open decisions
See `.agents/tasks/20261031-voice-dictation/HUMAN_INPUT.md`.
