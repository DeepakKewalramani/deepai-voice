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
