## What this PR does
Brings DeepAI Voice from the Phase 0 scaffold to a working dictation foundation: microphone capture with permission flow, local Whisper transcription with filters, hold-Fn hotkey, a notch pill overlay that never steals focus, typing-based paste with a secure-field guard, and a settings shell with permission and launch-at-login panels. Also adds an on-device streaming speech-to-text engine (slice 2.11).

## Why
These are the Phase 1 and Phase 2 slices needed before the demo dictation flow can be exercised end to end on this Intel Mac. They land together because they were built on the shared phase branch as one continuous batch.

## Scope
| Slice | Outcome | Built by | Reviewed by | Status |
|---|---|---|---|---|
| 1.1 | Mic permission + 16 kHz mono capture in memory | claude main + subagents | code-review batch | Ready |
| 1.3 | Silence, too-short, hallucination filters | claude main + subagents | code-review batch | Ready |
| 1.4 | SpeechToText trait + fallback chain + local adapter | claude main + subagents | code-review batch | Ready |
| 1.5 | Settings UI shell | claude main + subagents | code-review batch | Ready |
| 2.1 | Hold Fn + Ctrl+Option+Space fallback | claude main + subagents | code-review batch | Ready |
| 2.2 | Hold-to-talk wired end to end; Esc cancel; toggle mode still TODO | claude main | code-review batch | In progress |
| 2.3 | Notch pill overlay with four positions + drag | claude main + subagents | code-review batch | Ready |
| 2.4 | Paste at cursor (typing-based) + Accessibility check + secure field gate | claude main + subagents | code-review batch | Ready |
| 2.5 | Secure field block | claude main | code-review batch | Ready |
| 2.8 | Settings shell + permissions + launch-at-login panels | claude main | code-review batch | In progress |
| 2.11 | Streaming STT engine (on-device macOS Speech) | claude main | code-review batch | Ready |

**Not in this PR:** slice 2.6 LLM cleanup, 2.7 cloud STT adapters, 2.9 onboarding and .dmg build, 2.10 phase demo check, toggle mode shortcuts, provider keys management UI.

## How it works
Audio capture runs on a dedicated thread (AVFoundation, cpal probe fallback), feeds a 16 kHz mono buffer and a streaming resampler. Dictation is hold-to-talk: pressing and holding Fn records; live partials stream to the overlay while Whisper, queue behind an energy segmenter; release pastes typed text at the caret by synthesizing key events, guarded by a focus and secure-field check. The overlay is a borderless, non-activating window that reads audio level, state, and partial text, and can be dragged by simulated mouse events. The streaming engine wraps SFSpeechAudioBufferRecognitionRequest with on-device recognition and reports partial and final events.

## Changes by area
| Area | Change | Main files |
|---|---|---|
| Audio | permission status, in-memory capture, resampling, level meter | src-tauri/src/audio/ |
| STT | trait, filters, local whisper adapter, streaming + Apple Speech | src-tauri/src/stt/ |
| Dictation | hold state machine, live pipeline, Esc cancel, stream text | src-tauri/src/dictation/ |
| Paste | typing-based edit, focus guard, secure field stop | src-tauri/src/paste/, focus.rs |
| Overlay | notch pill, four positions, drag, tray reset | src-tauri/src/overlay.rs, overlay_drag.rs |
| Settings | permissions panel, launch at login, shell | src/settings/ |
| Integration | lib.rs wiring, tray menu, commands | src-tauri/src/lib.rs |

## Testing
| Check | Command | Result |
|---|---|---|
| Lint, format, typecheck, unit tests, frontend build, Rust fmt/clippy/test | `npm run check` | Pass, 73 vitest + cargo suites green on 65556e7 |
| Rust test suites | `cargo test` | All pass; streaming_test 14 pass, 1 ignored (permission prompt needs bundled app) |
| Secrets scan | `git diff` grep | No keys, env or transcripts in diff |
| Manual on Intel Mac | hotkey, paste, overlay drag, permissions panes | Not yet run; real Fn tap, real paste, real Speech recognition unverified |

## Review guide
1. `src-tauri/src/paste/typing.rs` and `focus.rs` first: the secure-field gate and typed edit guard.
2. `src-tauri/src/dictation/live.rs`: segmenter and hold pipeline.
3. `src-tauri/src/stt/streaming.rs` and `apple_speech.rs`: threading and event flow.
4. `src-tauri/src/overlay_drag.rs`: event tap and window drag.

## Review results
- **Fixed:** typed_bytes now counts u16 units typed, not UTF-8 bytes (review finding 7).
- **Left open (with reason):** nine other findings verified as false positives or intended conservative behavior: resampler loop drains each iteration; live.rs truncation always safe; overlay reset only on system tap-disabled re-enable; hotkey ctx freed after port; app-unreadable stop is deliberate; downmix callers always frame-aligned; stream_text never-delete is designed; overlay fraction guards zero range; settings migration bitwise OR ensures both raw keys removed.

## Risks and rollback
- **Risks:** real-device behavior (Fn tap, Accessibility typing, on-device Speech) verified only in unit tests, not on the live app.
- **Rollback:** revert this merge commit; no data or settings migration needed.
