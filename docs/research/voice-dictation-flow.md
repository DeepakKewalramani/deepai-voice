# Voice dictation: end-to-end user flow check

Date: 2026-10-31. Addendum job to `20261031-voice-dictation-gates.md`. Gates 1-3 only, no code, nothing installed.
Labels: FACT (verified against a primary source or local command, as recorded in `docs/research/voice-dictation-findings.md`), INFERENCE (likely, not confirmed), UNKNOWN (must be tested). "Spike Sx" and "step N" refer to `docs/specs/voice-dictation-plan.md`; "rule N" to `docs/specs/voice-dictation.md`.
Assumed stack: native Swift (D1 recommended default). Tauri differences are noted where they change a step.
Sources: F = docs/research/voice-dictation-findings.md, R0x = .agents/tasks/20261031-voice-dictation/research/0x-*.md.

Items marked **DEFAULT (pending user answer)** follow the recommended option in `.agents/tasks/20261031-voice-dictation/questions-draft.md` so the plan never stops on an open question.

## Summary

| Area | Status | Main gap |
|---|---|---|
| First launch, menu bar | INFERENCE | Bundle without Xcode untested (S0) |
| Mic permission | INFERENCE | Prompt timing, denied = silence (S0, step 14) |
| Accessibility permission | INFERENCE | Restart needed after grant? (UNKNOWN) |
| Hotkey press/release | FACT for Carbon press/release events (F §3); Fn UNKNOWN | S1 |
| Recording | INFERENCE | Device change mid-recording |
| Whisper | FACT that it runs on Intel CPU; speed UNKNOWN | S3 |
| Haiku cleanup | FACT for API constraints (F §7); latency UNKNOWN | S5 |
| Paste | INFERENCE | Delay per app, secure fields (S4) |
| History | Not in spec (spec excludes history UI) | Q5 |
| Error paths | Mostly INFERENCE | Listed below, each mapped to a test |

## 1. First launch

| # | Step | Label | Source | Check |
|---|---|---|---|---|
| 1.1 | User opens the `.app` from a fixed path (for example `~/Applications`). Bundle is built by SwiftPM plus a script, signed with one stable identity. | INFERENCE | F §1 (Xcode absent, CLT only), plan S0 | S0: bundle launches; `codesign -dv` shows the same identity after 2 rebuilds |
| 1.2 | No Dock icon, status item appears in the menu bar (`LSUIElement`). | INFERENCE | spec item 1; well-known Info.plist key, not re-fetched | Step 12 check |
| 1.3 | Gatekeeper: a locally built, non-notarized app launched on the same Mac. | INFERENCE: no quarantine flag on locally built files, so no Gatekeeper prompt. | general knowledge, not verified | S0: launch from Finder, note any dialog |
| 1.4 | App reads config (hotkey, cleanup on/off, model) with defaults. | INFERENCE (design) | spec | Phase 1 unit test of config defaults |
| 1.5 | App checks the API key: env `ANTHROPIC_API_KEY`, then Keychain. Finder-launched apps likely do not see shell env vars. | INFERENCE (downgraded) | F §8, R06 §1 | Step 11: launch from Finder with no Keychain item, menu shows "Cleanup unavailable" |
| 1.6 | Whisper model is loaded at startup with a warm-up run, so the first dictation is not slow. Model file path is fixed; missing model is an error path (E9). | INFERENCE (design) | plan step 9 | Step 9: time first vs second transcription |
| 1.7 | First-run notice says text goes to the Anthropic API when cleanup is on. **DEFAULT (pending user answer): cleanup on, with toggle and notice (D3/Q3).** | USER decision pending | HUMAN_INPUT D3 | Step 12: notice shown once, toggle persists |

## 2. Permission prompts

### 2a. Microphone
| # | Step | Label | Source |
|---|---|---|---|
| 2a.1 | Bundle must contain `NSMicrophoneUsageDescription`; without it the process may be killed or denied without prompt. | INFERENCE | F §4, R03 |
| 2a.2 | Hardened runtime needs `com.apple.security.device.audio-input` entitlement. Common for menu bar dictation apps (INFERENCE). | INFERENCE that we need it (only if hardened runtime is on). | F §2, R03 |
| 2a.3 | App calls `AVCaptureDevice.authorizationStatus(for: .audio)` at launch; if `notDetermined`, explains then calls `requestAccess`. Prompt otherwise appears on first capture. | INFERENCE | R03 |
| 2a.4 | Grant is tied to bundle id plus code signature; ad-hoc rebuilds may lose it. | INFERENCE | R03, R05 §9 |
| Check | S0 (grants survive rebuild), step 14 (`tccutil reset Microphone <bundle id>`, first-launch flow grants without restart). |

### 2b. Accessibility
| # | Step | Label | Source |
|---|---|---|---|
| 2b.1 | Needed to post synthetic Cmd+V and read the focused element role. | INFERENCE (Apple docs not fetched) | F §6, R05 §3 |
| 2b.2 | Silent check with `AXIsProcessTrusted()`, then an explainer, then the prompt variant and a deep link to Privacy_Accessibility. | INFERENCE | R05 §3 |
| 2b.3 | User toggles the switch in System Settings; app rechecks on activate or by polling. | INFERENCE | R05 §3 |
| 2b.4 | Whether the app must restart after the grant. | UNKNOWN | R05 §3 |
| 2b.5 | If never granted: app still works, copies text to clipboard and tells the user (rule 11). | Design | spec rule 11 |
| Check | Step 14 plus a new check: after grant, paste works with no restart; if it fails, document "restart required" in the explainer. |

### 2c. Input Monitoring
- Only needed if the trigger is Fn/Globe via a CGEventTap. Carbon `RegisterEventHotKey` needs no TCC permission (INFERENCE, F §3). **DEFAULT (pending user answer): normal key combo, so no Input Monitoring prompt (D2/Q2).**

## 3. Hotkey press
| # | Step | Label | Source |
|---|---|---|---|
| 3.1 | Global hotkey registered with Carbon `RegisterEventHotKey`; gives pressed and released events. | FACT (global-hotkey source uses it with both handlers; F §3). Swift path uses the same Carbon API: INFERENCE. | F §3 |
| 3.2 | **DEFAULT (pending user answer): hold Right Option or Ctrl+Option+Space** (exact combo picked in S1 from what does not clash). Fn optional spike. | INFERENCE | HUMAN_INPUT D2 |
| 3.3 | A modifier-only key (Right Option alone) cannot be a Carbon hotkey; it needs a `flagsChanged` monitor. NSEvent global monitors for key events need Accessibility. | INFERENCE | F §3 (Carbon needs a non-modifier key) |
| 3.4 | Key repeat while held must not restart recording (ignore repeated Pressed while recording). | INFERENCE (design) | none |
| 3.5 | Overlay: non-activating `NSPanel`, floating, ignores mouse; focus stays in the target app. | INFERENCE | plan S2 |
| 3.6 | Very short tap (under 300 ms) gives no paste (rule 2). | Design | spec rule 2 |
| 3.7 | Secure Event Input on (password field focused): event-tap based triggers may not fire; Carbon hotkeys may behave differently. | UNKNOWN | R05 §8 |
| Check | S1 (20/20 press and release detected), S2 (TextEdit keeps focus 10/10), extra S1 case: trigger with a password field focused. |

## 4. Recording
| # | Step | Label | Source |
|---|---|---|---|
| 4.1 | Engine starts on press: AVAudioEngine input tap at native rate (often 44.1 or 48 kHz), converted to 16 kHz mono f32 in memory. | INFERENCE (native rate not checked on this host) | R03 |
| 4.2 | First ~50 ms discarded (click). | INFERENCE | R03 |
| 4.3 | Level meter (RMS ~20 Hz) drives the overlay. | INFERENCE (design) | R03 |
| 4.4 | Memory: 64 KB per second at 16 kHz f32; 5 min = about 19 MB. | FACT (arithmetic) | R03 |
| 4.5 | macOS shows the orange mic indicator while recording. | INFERENCE | F §8 |
| 4.6 | Audio is never written to disk (rule 1). | Design | spec rule 1 |
| 4.7 | Default input device changes mid-recording (AirPods connect): stream may keep the old device or error. | INFERENCE | R03 |
| 4.8 | Bluetooth mic opens in a low-rate profile; native rate can change at start. | INFERENCE | R03 |
| Check | Step 8 (10 s gives 160000 samples +-1 percent; no file created), plus a manual AirPods connect test noted as a demo risk. |

## 5. Release
| # | Step | Label | Source |
|---|---|---|---|
| 5.1 | Released event stops the engine, flushes the converter, hands the buffer to the transcriber. | INFERENCE (design) | plan step 8 |
| 5.2 | Overlay switches to "Transcribing". Further presses while busy are ignored. **DEFAULT (pending user answer): ignore new presses until paste ends (Q6).** | Design | none |
| 5.3 | Frontmost app and focused element are recorded at release, to detect focus change before paste. | INFERENCE (design) | none |
| Check | Phase 1 test for the busy state; manual test: press again during transcription, nothing happens. |

## 6. Whisper (local)
| # | Step | Label | Source |
|---|---|---|---|
| 6.1 | whisper.cpp supports Intel Macs; Metal path documented only for Apple Silicon. Baseline is CPU (AVX2). | FACT for Intel support and Metal docs; INFERENCE (design) for the CPU AVX2 baseline |  F §5 |
| 6.2 | Host is Intel i9-9980HK, 32 GB. | FACT | F §1 |
| 6.3 | Metal on Radeon Pro 5300M works or helps. | UNKNOWN | F §5 |
| 6.4 | `base.en` q5_1 transcribes 10 s in under 1 s on this CPU. | UNKNOWN (target) | plan S3 |
| 6.5 | Hallucination filter drops "[BLANK_AUDIO]", lone "Thank you." on low RMS (rule 5). Local filler filter (rule 4). | Design | spec |
| Check | S3 timing table; Phase 1 tests 1 and 2. |

## 7. Haiku cleanup
| # | Step | Label | Source |
|---|---|---|---|
| 7.1 | Skip the call if cleanup is off, the key is missing, or the text is very short and clean. | INFERENCE (design) | R06 §4 |
| 7.2 | `POST /v1/messages`, model `claude-haiku-5-5`, transcript in `<transcript>` tags, fixed system prompt. | FACT for model id (F §7) | F §7, R06 |
| 7.3 | Do not send `temperature`/`top_p`/`top_k`; no assistant prefill. Both return 400. | FACT | F §7 |
| 7.4 | Haiku 5.5 thinks by default at effort `medium`; set `effort: low` and thinking disabled. Accepted together. | FACT for defaults and the "disabled only at high or below" rule; INFERENCE that the combination is accepted. | F §7, R06 §8 |
| 7.5 | Hard timeout `cleanupTimeoutMs` (2.0 s proposed), no retries. | INFERENCE (target) | R06 §4 |
| 7.6 | Latency p50/p95. | UNKNOWN | S5 |
| 7.7 | Sanity check on output (length ratio, preamble), else raw text (rules 6, 7). | Design | spec |
| 7.8 | Anthropic data retention for transcript text. | UNKNOWN | F §7 |
| Check | Phase 1 test 3 (stub client), S5 (20 real calls, cost under $0.01, needs approval). |

## 8. Paste
| # | Step | Label | Source |
|---|---|---|---|
| 8.1 | Check secure input (`IsSecureEventInputEnabled()`) and focused role `AXSecureTextField`; if either, do not paste (rule 9, fail closed). | INFERENCE | F §6, R05 §8 |
| 8.2 | If the frontmost app changed since release, still paste into the current app. **DEFAULT (pending user answer): paste into whatever is focused at paste time, matching common dictation tools (Q7).** | INFERENCE (design) | none |
| 8.3 | Save every pasteboard item and type, write text with `org.nspasteboard.TransientType`, confirm `changeCount`. | FACT for marker names (F §6); INFERENCE for API steps | F §6, R05 §5-6 |
| 8.4 | Post Cmd+V (keycode 9 with Command flag) via CGEvent. Non-US layouts: UNKNOWN. | INFERENCE / UNKNOWN | R05 §2b |
| 8.5 | Wait the per-app delay (50 to 300 ms, measured in S4), restore only if `changeCount` is still ours (rule 10). | INFERENCE | R05 §4 |
| 8.6 | Lazy (promised) clipboard data may not be restorable. | INFERENCE | R05 §5 |
| 8.7 | No trailing newline (rule 8). | Design | spec |
| Check | S4 matrix (TextEdit, Notes, Chrome, Slack, VS Code, Terminal, password field), Phase 1 tests 4 to 6, step 10. |

## 9. History
- Spec lists "history UI" under What NOT to build (FACT, `docs/specs/voice-dictation.md`).
- Gap: if paste fails, the user has no way to get the text back except the clipboard.
- **DEFAULT (pending user answer): no persisted history. Keep only the last result in memory, with a "Copy last dictation" menu item, cleared on quit (Q5).** Fits rule 1 spirit (no disk) and keeps scope small.
- Check: Phase 1 test (last result stored after a run, never written to disk), manual: quit and relaunch, menu item is disabled.

## 10. Error paths

| ID | Situation | Expected behavior | Label | Check |
|---|---|---|---|---|
| E1 | No microphone (no input device, Mac mini or lid closed with no external mic) | Engine start fails or `inputNode` has 0 channels; overlay shows "No microphone", no paste | INFERENCE | Unit test with a fake device provider returning no device (step 8) |
| E2 | Mic permission denied | Detect via `authorizationStatus == .denied` before recording; menu shows "Microphone blocked" with Settings link. Also detect long runs of exact zeros. | INFERENCE (denied may give silence) | `tccutil reset Microphone <id>`, deny, press hotkey |
| E3 | Accessibility not granted | No paste attempt; text copied to clipboard, notice shown (rule 11) | Design | Step 14 with permission off |
| E4 | Accessibility revoked while running | Check trust before every paste, not only at launch | INFERENCE (design) | Toggle off in Settings while app runs, dictate |
| E5 | Offline / DNS fail | Cleanup fails fast, raw filtered text pasted, menu shows "cleanup skipped, offline". Whisper works offline. | INFERENCE | Wi-Fi off, dictate (plan step 17) |
| E6 | API 401 / 429 / 5xx / timeout / refusal / max_tokens | Raw text pasted; status code logged, never the body (rule 12) | Design | Phase 1 test 3 |
| E7 | Empty audio (tap under 300 ms or silence) | No paste, overlay fades; no API call | Design (rules 2, 5) | Phase 1 test 2 |
| E8 | Long audio | Stop at `maxRecordSeconds`, warn at 90 percent (rule 3). **DEFAULT (pending user answer): 120 s cap (Q4).** Whisper processes 30 s windows; long text means longer Haiku output, `max_tokens` scales with input. | INFERENCE | Record past the cap; check auto-stop and full transcript |
| E9 | Model file missing or corrupt | Menu shows "Speech model missing", hotkey disabled | INFERENCE (design) | Rename model file, launch |
| E10 | Paste blocked in secure field | No paste, "Secure field, not pasted"; text kept in "Copy last dictation" only, not on the clipboard (avoid putting dictated text near a password). | Design (rule 9), INFERENCE for detection | S4 password field case, Safari login form, Terminal with Secure Keyboard Entry |
| E11 | Target app ignores Cmd+V (games, some remote desktop) | Text silently not inserted; clipboard restored. User recovers via "Copy last dictation". | INFERENCE | S4 matrix note |
| E12 | User copies during paste window | `changeCount` moved, do not restore (rule 10) | Design | Phase 1 test 6 |
| E13 | Hotkey already taken by another app | Registration fails; menu shows conflict and asks for another combo | INFERENCE (Carbon returns an error status) | S1: register a combo used by another app |
| E14 | Prompt injection in speech ("ignore previous instructions...") | Output treated as text, sanity check backstop (rule 13) | INFERENCE | R06 §7 test case 6 |
| E15 | Mic device change mid-recording | Stop recording with "Mic changed, try again" rather than mixing devices | INFERENCE | Manual AirPods test |

## 11. What this flow adds to the plan

These belong in `docs/specs/voice-dictation-plan.md` (owned by the first job; listed here for it to merge):
1. S1: add "trigger with a password field focused" and "combo already registered" cases.
2. Step 5 (busy state): new Phase 1 test, presses ignored while transcribing or pasting.
3. "Copy last dictation" in-memory last result (Q5 default) with its tests.
4. Step 14: verify whether Accessibility needs a restart after grant; trust checked before every paste (E4).
5. E1, E9, E15 handling in steps 8 and 9.
6. `maxRecordSeconds` default 120 (Q4) in config.

## 12. Open UNKNOWNs (all have a spike or check)
Fn capture (S1), Metal on AMD (S3), whisper speed (S3), Haiku latency and the effort plus disabled combination (S5), paste delay per app (S4), synthetic Cmd+V under Secure Event Input (S4), non-US keyboard layouts (S4), restart after Accessibility grant (step 14), API retention (read Anthropic commercial terms before demo).
