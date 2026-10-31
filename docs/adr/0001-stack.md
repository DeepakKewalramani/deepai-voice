# ADR 0001: Application stack for the voice dictation demo

- Status: Accepted (D1 answered by the user on 31 Oct 2026). D2 to D4 and new questions are still open.
- Date: 2026-10-31
- Sources: docs/research/voice-dictation-findings.md, docs/research/cross-platform-findings.md, .agents/tasks/20261031-voice-dictation/research/02, 07, 09 to 14

## Decision change record
- Previous: native Swift (SwiftPM, scripted .app bundle), macOS only.
- New: Tauri v2 + Rust, one codebase for macOS and Windows.
- Reason: user decision on 31 Oct 2026. The app must support macOS and Windows from one codebase. The demo is on the Mac first. The Windows code path is written now and tested later on a Windows machine.
- Evidence: USER-PROVIDED (handoff 20261031-voice-dictation-crossplatform). Feasibility checked in research 09 to 14.
- Scope: this project only.

## Context
We need a tray / menu bar app that records speech while a key is held, transcribes it locally with whisper.cpp, cleans the text with Claude Haiku (`claude-haiku-5-5`, key from `ANTHROPIC_API_KEY`) and pastes it into the frontmost app. Demo quality.

Host facts (checked with sysctl, sw_vers and version commands): Intel i9-9980HK, x86_64, 32 GB RAM, macOS 15.7.4, cargo/rustc, cmake and clang installed, Command Line Tools only (no full Xcode). Windows machine: none yet.

## Decision
Tauri v2 + Rust, single crate, `core/` (pure, shared) plus a `Platform` trait with `macos.rs` and `windows.rs` selected by `cfg(target_os)` (design: research/14).
- Shell: tauri 2.12.x, `tray-icon` feature, macOS `ActivationPolicy::Accessory` (no Dock icon). FACT, research/09.
- Hotkey: tauri-plugin-global-shortcut 2.4.x for a key combo (Pressed and Released on both OSes; on Windows Released is emulated by polling). Modifier-only keys and Fn are not supported by the plugin and would need a per-OS native hook. FACT/INFERENCE, research/09.
- Overlay: Tauri window with `focusable(false)`, always on top, click-through. Known open focus bugs on both OSes (tauri#14102, #14386), so it gets a spike. tauri-nspanel is the macOS fallback.
- Audio: cpal 0.18 at the device rate, resampled to 16 kHz mono with rubato 5. FACT, research/10.
- STT: whisper-rs 0.16 (whisper.cpp 1.8.3, static, CPU baseline; AVX2 flags set explicitly for distributable builds). FACT, research/10.
- Paste: own small paste function behind the trait: CGEvent Cmd+V on macOS, SendInput Ctrl+V on Windows. Clipboard text via arboard 3.7 plus per-OS marking (Concealed via arboard on macOS, Transient via direct NSPasteboard call if needed; history and cloud exclusion formats on Windows). FACT/INFERENCE, research/11.
- Secure field check: macOS `IsSecureEventInputEnabled` plus focused AX subrole; Windows UI Automation `IsPassword`. Unknown result means do not paste. INFERENCE, research/11.
- API key: `ANTHROPIC_API_KEY` env var first, then the OS credential store via the keyring crate (version UNKNOWN). Calls: reqwest to the Messages API.

## Alternatives
| Option | Pros | Cons | Verdict |
|---|---|---|---|
| A. Tauri v2 + Rust (chosen) | One codebase for Mac and Windows. Crates exist for every part (research/09, 10, 11). Rust toolchain already installed. | Two languages (Rust + small web UI). Overlay focus bugs open. Per-OS FFI for paste, secure field, Fn. | Chosen: only option that meets the Mac + Windows requirement without a second app. |
| B. Native Swift | Direct macOS APIs, one language, small binary. | macOS only. Windows would need a second app in another language. | Rejected 31 Oct 2026: fails the cross-platform requirement. |
| C. Electron | Fast UI work, cross-platform. | 150 MB+ bundle, still needs native addons for hotkey release, paste, overlay and secure field. | Rejected: same native work as A plus a heavier runtime. |
| D. Swift on Mac + separate C#/WinUI on Windows | Best native fit per OS. | Two codebases, double the work and tests. | Rejected: violates "one codebase". |

## Consequences
- Per-OS code is limited to the `Platform` trait: clipboard snapshot and restore, paste keystroke, secure field check, overlay quirks, permissions. Everything else is shared and tested on the Mac.
- Windows parts are compiled and reviewed but not run until a Windows machine is available. Whether they compile from the Mac (`cargo check --target x86_64-pc-windows-msvc`, cargo-xwin, or CI on windows-latest) is UNKNOWN and gets a spike (W0).
- Building whisper-rs on Windows needs MSVC, CMake and LLVM (`LIBCLANG_PATH`). FACT, research/10.
- Windows builds are unsigned for now. SmartScreen shows "Run anyway"; Smart App Control on Windows 11 can block them. Signing path is an open question (Q12).
- macOS: ad-hoc signing (`signingIdentity "-"`). TCC grants may reset when the signature changes (INFERENCE, not confirmed in Apple docs), so the S0 spike checks rebuilds. No notarization for the demo.
- Requires Rust 1.90 or newer (global-shortcut plugin MSRV). Installed: 1.99.1 (FACT, checked 31 Oct 2026).
- Original work only. No copied code, assets, prompts or branding from any other product.

## Addendum (31 Oct 2026, user decision): frontend and comparison
- Frontend: React + TypeScript + Vite inside Tauri v2. Rust only for native parts (mic, Whisper, hotkey release, paste, secure field). Reason: user is a full-stack web developer; keeps most code in TypeScript while the app stays small.
- Also compared and rejected (scores are judgment estimates, not measurements): Wails/Go 79% (less mature tray and overlay), Electron 79% (heavy), Electrobun 65% (young), Neutralino 64% (no hold-to-talk or native paste), C++ 61%, NW.js 59%, Python/pywebview 57%, .NET 54%. Tauri 87%.
