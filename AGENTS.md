# AGENTS.md: rules for every AI agent on DeepAI Voice

This is the single source of truth for any AI tool working here: Claude Code, Codex, Cursor, Gemini CLI, Copilot, Antigravity, claude-free, or any other. Tool-specific files (CLAUDE.md, GEMINI.md, .cursor/rules, .github/copilot-instructions.md) only point here.

## 1. What we are building
DeepAI Voice: our own product. Dictation, live translator (any language, suggested replies, speak into the call), meeting notes (any call app). Mac first, Windows later. Nothing copied from Wispr Flow or any other product.

## 2. Read before any work (in this order)
1. `AGENTS.md` (this file)
2. `docs/WORKFLOW.md` (the flow every agent follows)
3. `docs/ROADMAP.md` (phases and slices)
4. `docs/tasks/BOARD.md` (who is doing what right now)
5. The slice file you are assigned in `docs/tasks/`
6. Related `docs/specs/`, `docs/adr/`, `docs/INSTALLED.md`

## 3. Stack (decided, ADR 0001, do not change without a new ADR)
Tauri v2 + Rust for native parts only (audio, Whisper, hotkey, paste, secure field, system audio). React + TypeScript + Vite for UI and app logic. Providers behind traits: `stt`, `llm`, `tts`.

## 4. Hard rules
- Work on ONE slice at a time, only after claiming it on `docs/tasks/BOARD.md`. Never edit files another agent's claimed slice owns.
- Small slices: target under ~300 changed lines, working and testable on its own.
- Test first where possible. Run tests, lint, typecheck, build. Paste real output in the slice file. Never claim "works" without it.
- Never weaken or delete a failing test to make it pass.
- Never push directly to `main`. Every change reaches `main` only through a reviewed PR. Branch per phase: `phase-<n>-<name>`. Slice branch: `phase-<n>/<slice-id>-<short>`. Clean squashed commits.
- Push, PR creation and merge need NO approval phrase in this project (user override, 31 Oct 2026; replaces the global APPROVE PUSH / APPROVE MERGE rule here only). Still: never push directly to `main` (PR only), never force push, review and tests must pass first. Installs, downloads and paid API calls still need the human's OK. Log every install in `docs/INSTALLED.md`.
- Never log or commit API keys, audio, transcripts, translations or `.env`. Keys only in Keychain.
- Label claims FACT (verified, with source or command), INFERENCE, UNKNOWN.
- No em-dashes in docs, commits or PRs. No `Co-Authored-By` trailer, no "Generated with" lines.
- Questions only the human can answer go to `HUMAN_INPUT.md` with a recommended default. Continue other safe work.

## 5. Commands
Run from the repo root.

Checks (must pass before every PR). `npm run check` runs these, in this order, and stops at the first failure:
| Command | What it does |
|---|---|
| `npm run lint` | ESLint on `src/` and root config files |
| `npm run format` | Prettier check (`npm run format:write` to fix; markdown excluded) |
| `npm run typecheck` | `tsc --noEmit` for `tsconfig.json` (app) and `tsconfig.node.json` (vite.config.ts) |
| `npm run test` | Vitest unit tests |
| `npm run build` | Frontend build (`tsc && vite build`) |
| `npm run rust:fmt` | `cargo fmt --check` |
| `npm run rust:lint` | `cargo clippy --all-targets -- -D warnings` |
| `npm run rust:test` | `cargo test` |

Pre-merge release check (not in `check`, slow): `npm run tauri build -- --debug`, bundle at `src-tauri/target/debug/bundle/macos/DeepAI Voice.app`, then launch it.

Not a check: `npm install` (setup), `npm run tauri dev` (dev run with hot reload).

## 6. PR title and description format
- Title: `Phase <n>: <what the user gets>` for phase PRs, or `<slice id>: <what it does>` for single slices. Plain words, no file names, no "fix stuff", no "WIP".
  Examples: `Phase 0: App scaffold, project checks and coordination tools`, `1.2: Local Whisper speech-to-text`.
- Description: use `.github/pull_request_template.md` exactly (Summary, Slices, Changes, How it was tested with real results, Review, Not verified). Write it once from verified facts; update only when new slices land. No claims without a command result. No "Generated with" line.
- Commits: `Slice <id>: <what it does>` or a short imperative sentence; body explains why when not obvious.
