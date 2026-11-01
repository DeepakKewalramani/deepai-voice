# Installed tools and downloads (deepAi)

Every install or download for this project is logged here, so it can be removed cleanly later.
Rule: nothing system-wide unless needed. No full Xcode (user decision D4, 31 Oct 2026).

## Already on the machine (not installed by this project)
| Tool | Version | Location |
|---|---|---|
| Xcode Command Line Tools | system | /Library/Developer/CommandLineTools |
| Rust (cargo, rustc) | 1.99.1 | Homebrew |
| Node / npm | 24.15.0 / 11.12.1 | system |
| CMake | 4.4.4 | system |
| Homebrew | 7.0.11 | system |

## Installed or downloaded for this project
| Date | What | Why | Where | Size | How to remove |
|---|---|---|---|---|---|
| 31 Oct 2026 | Whisper model `ggml-base.en.bin` | Local speech-to-text (Q10, approved by user) | `deepAi/models/` (project only, not system-wide) | 148 MB (147,964,211 bytes). Source: huggingface.co/ggerganov/whisper.cpp. sha256 a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002 (not yet compared with upstream) | `rm deepAi/models/ggml-base.en.bin` |

## Not installed (by decision)
- Full Xcode (D4).
- whisper.cpp via Homebrew (built inside the Rust project instead, no system install).

## Project dependencies (slice 0.2, project-local, remove with `rm -rf node_modules src-tauri/target`)
| Date | Package | Version | Where |
|---|---|---|---|
| 31 Oct 2026 | Scaffold generator create-tauri-app (run once via npm create, not kept) | 4.7.5 | npm cache only |
| 31 Oct 2026 | @tauri-apps/cli (dev), @tauri-apps/api (runtime JS API for IPC) | 2.12.4 | node_modules |
| 31 Oct 2026 | react, react-dom | 19.3.1 | node_modules |
| 31 Oct 2026 | vite, @vitejs/plugin-react | 8.3.5, 6.1.2 | node_modules |
| 31 Oct 2026 | typescript | 6.0.3 | node_modules |
| 31 Oct 2026 | tauri (crate, feature tray-icon), tauri-build | 2.12.5, 2.7.4 | src-tauri/target, Cargo.lock |
| 31 Oct 2026 | serde, serde_json (crates, kept for slice 0.4 settings) | 1.0.229, 1.0.154 | src-tauri/target, Cargo.lock |
| 31 Oct 2026 | eslint, @eslint/js (slice 0.3) | 10.14.0, 10.0.1 | node_modules |
| 31 Oct 2026 | typescript-eslint, eslint-plugin-react-hooks, globals (slice 0.3) | 8.72.1, 7.1.1, 17.13.0 | node_modules |
| 31 Oct 2026 | prettier (slice 0.3) | 3.9.10 | node_modules |
| 31 Oct 2026 | vitest (slice 0.3) | 5.0.5 | node_modules |
| 31 Oct 2026 | keyring (crate, features apple-native, windows-native; slice 0.4, 3.x chosen over 4.2.0 for the stable Entry API) | 3.6.3 | src-tauri/target, Cargo.lock |
| 31 Oct 2026 | tracing, tracing-subscriber, regex (slice 0.5, redacted logging) | 0.1, 0.3, 1.12 | src-tauri/target, Cargo.lock |

Note: `npm audit` reports 11 moderate findings in dev dependencies (31 Oct 2026). Not triaged in 0.3.

## Project dependencies (slice 1.2)
| Date | Package | Version | Where |
|---|---|---|---|
| 31 Oct 2026 | whisper-rs (crate, builds whisper.cpp via whisper-rs-sys 0.15.0, cmake, bindgen) | 0.16.0 | src-tauri/target, Cargo.lock |
| 31 Oct 2026 | hound (dev crate, reads the WAV fixture in tests) | 3.5.1 | src-tauri/target, Cargo.lock |

## Project dependencies (slice 1.1)
| Date | Package | Version | Where |
|---|---|---|---|
| 31 Oct 2026 | cpal (crate, audio capture from input device) | 0.18.2 | src-tauri/target, Cargo.lock |
| 31 Oct 2026 | rubato (crate, audio resampling to 16 kHz) | 5.0.1 | src-tauri/target, Cargo.lock |

## Project dependencies (slice 2.4)
| Date | Package | Version | Where |
|---|---|---|---|
| 31 Oct 2026 | arboard (crate, clipboard read/write; clipboard access only, not for audio) | 3.x | src-tauri/target, Cargo.lock |
| 31 Oct 2026 | objc2 0.6.5, objc2-av-foundation 0.3.2, block2 0.6.2 (macOS target only; Zlib OR Apache-2.0 OR MIT; block2 is direct because the permission completion handler needs RcBlock) | real AVFoundation mic permission status and request (slice 1.1), approved by human | src-tauri/Cargo.toml, Cargo.lock |
| 31 Oct 2026 | tauri-plugin-global-shortcut 2.4.1 (crate; Apache-2.0 OR MIT; official Tauri plugin) | Ctrl+Option+Space fallback hold-to-talk hotkey (slice 2.1), approved by human | src-tauri/Cargo.toml, Cargo.lock |
| 1 Nov 2026 | objc2-speech 0.3.2 and objc2-avf-audio 0.3.2 (Zlib OR Apache-2.0 OR MIT), objc2-foundation 0.3.2 (MIT); crates, exact pins, macOS target only, minimal features | on-device streaming speech to text via the macOS Speech framework and 16 kHz float32 audio buffers (slice 2.11), approved by human | src-tauri/Cargo.toml, Cargo.lock |
