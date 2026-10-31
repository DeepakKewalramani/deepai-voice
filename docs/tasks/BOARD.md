# Task board (the lock, see docs/WORKFLOW.md sections 5 and 8)

Pick your next task automatically: `node scripts/next-task.mjs <lane>` (lane = claude or antigravity).
Lanes: `claude` = Rust native and review heavy; `antigravity` = UI, TypeScript, filters and tests; `any` = whoever is free.
A slice is offered only when every Depends slice is DONE and its Owns paths do not overlap any IN PROGRESS or IN REVIEW slice.

| Slice | Title | Status | Owner agent | Branch | Updated | Lane | Depends | Owns |
|---|---|---|---|---|---|---|---|---|
| 0.1 | git init, .gitignore, README, docs map | DONE | Claude main | main | 31 Oct 2026 | any | - | README.md, .gitignore |
| 0.2 | Tauri + React + TS + Vite scaffold, menu bar app | DONE https://github.com/DeepakKewalramani/deepai-voice/pull/2 | Claude subagent builder | phase-0/0.2-scaffold | 2026-10-31 13:21 | claude | 0.1 | package.json, src/, src-tauri/, vite.config.ts, index.html, tsconfig |
| 0.3 | Lint, format, typecheck, tests, scripts | DONE https://github.com/DeepakKewalramani/deepai-voice/pull/2 | Claude subagent builder | phase-0/0.2-scaffold | 2026-10-31 13:21 | claude | 0.2 | package.json, eslint.config, .prettierrc |
| 0.4 | Settings store + Keychain | IN REVIEW | claude-builder-settings | phase-0-foundation | 2026-10-31 13:40 | claude | 0.3 | src-tauri/src/settings/ |
| 0.5 | Redacted logging | IN REVIEW | antigravity-builder | phase-0-foundation | 2026-10-31 13:40 | antigravity | 0.3 | src-tauri/src/logging/ |
| 1.1 | Mic permission + 16 kHz mono capture in memory | TODO |  |  |  | claude | 0.4, 0.5 | src-tauri/src/audio/ |
| 1.2 | Whisper base.en load once + transcribe fixture | IN REVIEW | claude-builder-whisper | phase-0-foundation | 2026-10-31 13:40 | claude | 0.3 | src-tauri/src/stt/local_whisper/ |
| 1.3 | Silence, too-short, hallucination filters | TODO |  |  |  | antigravity | 1.2 | src-tauri/src/stt/filters/ |
| 1.4 | SpeechToText trait + local adapter | TODO |  |  |  | claude | 1.2, 1.3 | src-tauri/src/stt/mod.rs |
| 1.5 | Settings UI shell (React) | TODO |  |  |  | antigravity | 0.4 | src/settings/ |
| P.1 | Write slice files for Phase 1 (1.1 to 1.5) from _TEMPLATE.md | IN REVIEW | antigravity-planner | phase-0-foundation | 2026-10-31 13:40 | antigravity | 0.1 | docs/tasks/1. |
| P.2 | Write slice files for Phase 2 dictation (2.1 to 2.10) from ROADMAP | IN REVIEW | antigravity-planner | phase-0-foundation | 2026-10-31 13:40 | antigravity | 0.1 | docs/tasks/2. |
| P.3 | Test audio fixtures plan + 5 short WAV fixtures (own voice or generated with macOS say), 16 kHz mono, with expected text | IN REVIEW | antigravity-tester | phase-0-foundation | 2026-10-31 13:40 | antigravity | 0.1 | tests/fixtures/audio/ |

The Planner adds the next phase's slices when the current phase is half done. Full list: docs/ROADMAP.md.
