# Task board (the lock, see docs/WORKFLOW.md sections 5 and 8)

Pick your next task automatically: `node scripts/next-task.mjs <lane>` (lane = claude or antigravity).
Lanes: `claude` = Rust native and review heavy; `antigravity` = UI, TypeScript, filters and tests; `any` = whoever is free.
A slice is offered only when every Depends slice is DONE and its Owns paths do not overlap any IN PROGRESS or IN REVIEW slice.

| Slice | Title | Status | Owner agent | Branch | Updated | Lane | Depends | Owns |
|---|---|---|---|---|---|---|---|---|
| 0.1 | git init, .gitignore, README, docs map | DONE | Claude main | main | 31 Oct 2026 | any | - | README.md, .gitignore |
| 0.2 | Tauri + React + TS + Vite scaffold, menu bar app | DONE https://github.com/DeepakKewalramani/deepai-voice/pull/2 | Claude subagent builder | phase-0/0.2-scaffold | 2026-10-31 13:21 | claude | 0.1 | package.json, src/, src-tauri/, vite.config.ts, index.html, tsconfig |
| 0.3 | Lint, format, typecheck, tests, scripts | DONE https://github.com/DeepakKewalramani/deepai-voice/pull/2 | Claude subagent builder | phase-0/0.2-scaffold | 2026-10-31 13:21 | claude | 0.2 | package.json, eslint.config, .prettierrc |
| 0.4 | Settings store + Keychain | DONE https://github.com/DeepakKewalramani/deepai-voice/pull/3 | claude-builder-settings | phase-0-foundation | 2026-10-31 13:55 | claude | 0.3 | src-tauri/src/settings/ |
| 0.5 | Redacted logging | DONE https://github.com/DeepakKewalramani/deepai-voice/pull/3 | antigravity-builder | phase-0-foundation | 2026-10-31 13:55 | antigravity | 0.3 | src-tauri/src/logging/ |
| 1.1 | Mic permission + 16 kHz mono capture in memory | REVIEWED, committed 5a74c9a (real mic prompt not verified) | claude main + subagents | phase-1/1.1 | 2026-11-01 00:15 | claude | 0.4, 0.5 | src-tauri/src/audio/ |
| 1.2 | Whisper base.en load once + transcribe fixture | DONE https://github.com/DeepakKewalramani/deepai-voice/pull/3 | claude-builder-whisper | phase-0-foundation | 2026-10-31 13:55 | claude | 0.3 | src-tauri/src/stt/local_whisper/ |
| 1.3 | Silence, too-short, hallucination filters | REVIEWED, committed d605eab | claude main + subagents | phase-1/1.3 | 2026-11-01 00:15 | antigravity | 1.2 | src-tauri/src/stt/filters/ |
| 1.4 | SpeechToText trait + local adapter | REVIEWED, committed 8835549 | claude main + subagents | phase-0-foundation | 2026-11-01 00:15 | claude | 1.2, 1.3 | src-tauri/src/stt/mod.rs, src-tauri/src/stt/trait_def.rs, src-tauri/src/stt/chain.rs, src-tauri/src/stt/local_adapter.rs |
| 1.5 | Settings UI shell (React) | REVIEWED, committed a98da0a (mounted in App 32b1480; settings UI reworked in 1bbbf95) | claude main + subagents | phase-1/1.5 | 2026-11-01 12:30 | antigravity | 0.4 | src/settings/ |
| P.1 | Write slice files for Phase 1 (1.1 to 1.5) from _TEMPLATE.md | DONE https://github.com/DeepakKewalramani/deepai-voice/pull/3 | antigravity-planner | phase-0-foundation | 2026-10-31 13:55 | antigravity | 0.1 | docs/tasks/1. |
| P.2 | Write slice files for Phase 2 dictation (2.1 to 2.10) from ROADMAP | DONE https://github.com/DeepakKewalramani/deepai-voice/pull/3 | antigravity-planner | phase-0-foundation | 2026-10-31 13:55 | antigravity | 0.1 | docs/tasks/2. |
| P.3 | Test audio fixtures plan + 5 short WAV fixtures (own voice or generated with macOS say), 16 kHz mono, with expected text | DONE https://github.com/DeepakKewalramani/deepai-voice/pull/3 | antigravity-tester | phase-0-foundation | 2026-10-31 13:55 | antigravity | 0.1 | tests/fixtures/audio/ |
| 2.4 | Paste at cursor + clipboard restore + Accessibility check | REVIEWED, committed ab703fc (clipboard approach superseded by typing paste 3d251ae; real paste not verified) | claude main + subagents | phase-0-foundation | 2026-11-01 12:30 | claude | 1.4 | src-tauri/src/paste/, src-tauri/tests/paste_test.rs, src-tauri/Cargo.toml, docs/INSTALLED.md, docs/tasks/2.4.md |
| 2.1 | Hold Fn + Ctrl+Option+Space fallback | REVIEWED, committed 492b90a (real Fn tap not verified) | claude main + subagents | phase-0-foundation | 2026-11-01 00:15 | claude | 1.1, 1.4 | src-tauri/src/hotkey/, src-tauri/tests/hotkey_test.rs, src-tauri/Cargo.toml, docs/tasks/2.1.md |
| 2.2 | Toggle/hold dictation modes + separate shortcuts | IN PROGRESS (hold-to-talk wired end-to-end e8773a6, Esc cancel 2fbfe6a; toggle mode not yet) | claude main | phase-0-foundation | 2026-11-01 12:30 | claude | 2.1 | src-tauri/src/hotkey/toggle.rs, config.rs, src-tauri/tests/hotkey_toggle_test.rs, docs/tasks/2.2.md |
| 2.3 | Floating control bar (pill overlay) | IN REVIEW, committed (notch pill f4ba050, idle state de814e3, drag 3d967df) | claude main + subagents | phase-0-foundation | 2026-11-01 12:30 | antigravity | 0.2, 2.1 | src/overlay/, src-tauri/src/overlay/, src-tauri/src/overlay_drag.rs |
| 2.5 | Secure field block | IN REVIEW, committed 3d251ae (secure-field gate in focus.rs/typing.rs) | claude main | phase-0-foundation | 2026-11-01 12:30 | claude | 2.4 | src-tauri/src/paste/secure.rs, src-tauri/tests/secure_field_test.rs, docs/tasks/2.5.md |
| 2.8 | Settings UI: providers, keys, languages, prompts, shortcuts; usage estimate; privacy notices | IN PROGRESS (shell + permissions + launch-at-login a5a2f37; provider keys section still missing) | claude main | phase-0-foundation | 2026-11-01 12:30 | antigravity | 0.4 | src/settings/ |
| 2.11 | Streaming STT engine (on-device macOS Speech) | IN REVIEW, committed 5811558 (streaming_test 14 pass, 1 ignored; real recognition not verified) | claude main | phase-0-foundation | 2026-11-01 12:30 | claude | 2.1 | src-tauri/src/stt/streaming.rs, apple_speech.rs, src-tauri/tests/streaming_test.rs |

The Planner adds the next phase's slices when the current phase is half done. Full list: docs/ROADMAP.md.
