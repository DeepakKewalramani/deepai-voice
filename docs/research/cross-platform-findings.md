# Cross-platform findings (macOS first, Windows later)

Date: 2026-10-31. Detail and source URLs: `.agents/tasks/20261031-voice-dictation/research/09-tauri-xplat.md`, `10-audio-whisper-xplat.md`, `11-paste-xplat.md`, `12-windows-perms.md`, `13-apple-docs.md`, `14-architecture.md`.
Labels: FACT (checked against a source), INFERENCE (likely, not confirmed), UNKNOWN (needs a spike or a decision).

## Versions (FACT, crates.io, 31 Oct 2026)
| Crate | Version | Note |
|---|---|---|
| tauri | 2.12.5 | `tray-icon` feature for the tray |
| tauri-plugin-global-shortcut | 2.4.1 | MSRV Rust 1.90 |
| tauri-nspanel | 2.1.1 | third-party, macOS only |
| cpal | 0.18.2 | MSRV 1.85; API breaks vs 0.15 |
| rubato | 5.0.1 | `audioadapter` buffers |
| whisper-rs / whisper-rs-sys | 0.16.0 / 0.15.0 | bundles whisper.cpp 1.8.3, static link; MSRV 1.88 is declared by whisper-rs-sys, whisper-rs declares none |
| arboard | 3.7.0 | one content kind per write |
| enigo | 0.6.1 | self-described early alpha; not chosen |
| keyring | UNKNOWN | version and features not checked |

## Hotkey
- FACT: the plugin reports Pressed and Released on macOS (native Carbon event) and Windows (Released emulated by polling GetAsyncKeyState about every 50 ms). (09)
- UNKNOWN: whether the global-hotkey version pinned by plugin 2.4.1 contains that Windows polling fix. (09)
- FACT: neither backend maps Right Option, Right Alt or Fn alone. Modifier-only triggers need a native path (macOS flagsChanged / event tap, Windows WH_KEYBOARD_LL). The native paths are INFERENCE. (09)
- INFERENCE: RegisterHotKey and WH_KEYBOARD_LL need no admin rights (the Microsoft docs list no such requirement but do not state it). (12)

## Tray and overlay
- FACT: TrayIconBuilder supports a menu and events; `ActivationPolicy::Accessory` hides the Dock icon on macOS. (09)
- FACT: on Windows, `focusable(false)` becomes WS_EX_NOACTIVATE in tao. (09)
- FACT: open bugs: tauri#14102 (macOS non-focusable window still takes focus on click) and tauri#14386 (Windows non-focusable window gets no key events). Click-through likely avoids #14102 (INFERENCE). (09)
- UNKNOWN: tauri-nspanel non-activating style mask support. Spike S2.

## Audio and whisper
- FACT: cpal uses CoreAudio (min macOS 14.2) and WASAPI (min Windows 8); no CMake or LLVM needed for cpal. Default input prefers 48 kHz then 44.1 kHz; sample format may be I32/I24. (10)
- INFERENCE: resample to 16 kHz with rubato on both OSes rather than trusting WASAPI auto-convert. (10)
- FACT: whisper-rs default features are none (CPU). Optional: metal, vulkan, cuda, openblas, coreml. Static link. (10)
- INFERENCE (the ggml options exist, FACT): for a build that runs on other CPUs set `GGML_NATIVE=OFF GGML_AVX2=ON GGML_FMA=ON GGML_F16C=ON`. (10)
- FACT: Windows build needs Visual Studio C++ tools, CMake and LLVM with `LIBCLANG_PATH`; MSVC on ARM unsupported; vulkan build panics without `VULKAN_SDK`. (10)
- UNKNOWN: Metal speed on the Radeon Pro 5300M. CPU stays the baseline.
- FACT: model sizes: base.en-q5_1 59,721,011 bytes; small.en-q5_1 190,098,681; tiny.en-q5_1 32,166,155. Same file works on both OSes. (10)

## Paste and clipboard
- FACT: arboard cannot enumerate or copy every clipboard format, so a full multi-format snapshot and restore is not possible with it. Best-effort restore of one kind (text, html, image or file list), or per-OS native code. (11)
- FACT: arboard on macOS can mark a write Concealed (`exclude_from_history`), not Transient. On Windows it can set the monitoring, cloud and history exclusion formats (confirmed on Microsoft Learn). (11)
- FACT: macOS `NSPasteboard.changeCount` increments on each ownership change; Apple suggests saving it after your write and comparing later. (13) Windows equivalent: `GetClipboardSequenceNumber`. (11)
- FACT: SendInput into a higher-integrity (elevated) window is blocked by UIPI and neither the return value nor GetLastError says so. (11, 12) Plan: detect elevation of the target ourselves, skip the paste, keep the text for "Copy last dictation".
- FACT: uiAccess needs signing, a manifest flag and install under Program Files, and Microsoft says only assistive apps should use it. Not used. (11)
- INFERENCE: own small paste function (CGEvent / SendInput) instead of enigo, so we control failure detection. (11)

## Secure fields
- FACT: `kAXSecureTextFieldSubrole` is the subrole for a text field that shows bullets. (13)
- UNKNOWN: Apple doc page for `IsSecureEventInputEnabled` returned 404; semantics from header and runtime test. (11, 13)
- FACT: Windows UI Automation exposes `IsPassword`. INFERENCE: custom-drawn controls, remote desktops and elevated windows are blind spots, so an unknown result blocks the paste. (11)

## Permissions
- FACT: `AXIsProcessTrustedWithOptions` returns whether the process is trusted; the prompt option shows the system prompt asynchronously. (13)
- UNKNOWN: whether the app needs a restart after Accessibility is granted. Not in Apple docs fetched. Runtime check. (13)
- UNKNOWN: whether Apple docs state that CGEventPost needs Accessibility. The CGEventPost page does not say. Runtime check. (13)
- FACT: `AVCaptureDevice.requestAccess` throws if `NSMicrophoneUsageDescription` is missing; audio is silent until access is granted. (13)
- UNKNOWN: TCC grants tied to the designated requirement (page had no body). Spike S0. (13)
- FACT: Windows desktop apps share one switch, "Let desktop apps access your microphone"; deep link `ms-settings:privacy-microphone`. (12)
- UNKNOWN: how a desktop app learns that mic access is denied on Windows. INFERENCE: detect silence or open failure. (12)

## Packaging and signing
- FACT: macOS bundle (.app/.dmg) must be built on a Mac; ad-hoc signing is `signingIdentity "-"`; notarization needs a paid Developer ID. (09)
- FACT: Windows MSI uses WiX and builds only on Windows; NSIS can be cross-built but the docs call it less tested; NSIS installs per-user by default. WebView2 is preinstalled on Windows 11 and recent Windows 10. (09)
- FACT: unsigned Windows builds show the SmartScreen "Run anyway" screen; Smart App Control on Windows 11 can block them; EV certificates no longer give instant SmartScreen reputation. (09, 12)
- FACT: Microsoft Artifact Signing (formerly Trusted Signing) price stated as $9.99/month in research 12 but not found on the re-fetched Learn FAQ or quickstart (UNKNOWN); public trust is limited to organisations in listed countries, and India is not listed. Tequity's legal entity location is UNKNOWN. (12)
- INFERENCE: keyboard hook plus SendInput may trigger antivirus heuristics on unsigned builds; evidence is weak and third-party. (12)

## Open items that need a Windows machine
Mic denial behaviour, Released timing of the plugin, overlay focus (#14386), SendInput vs elevated targets, UIA IsPassword latency and COM threading, installer signing, a Defender scan of an unsigned build.
