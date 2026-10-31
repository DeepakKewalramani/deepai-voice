# Human input required

Questions that only the human can answer. Each question includes a recommended default.
Continue other safe work while waiting; do not block on these.

---

## [Slice 2.1] Approve installation of tauri-plugin-global-shortcut

**Question**: May we add `tauri-plugin-global-shortcut` as a Cargo dependency
to enable the Ctrl+Option+Space fallback hotkey on macOS and Windows?

**What it does**: Registers a system-wide keyboard shortcut via the OS global
shortcut API. Required for the fallback path when Fn (Globe) key is not
available or when the user is on Windows.

**Crate**: `tauri-plugin-global-shortcut` (official Tauri plugin, MIT license)
**Cargo.toml addition** (additive, one line):
```toml
tauri-plugin-global-shortcut = "2"
```

**Recommended default**: YES - approve the install. The crate is an official
Tauri plugin, MIT-licensed, and does not make network calls.

**Answer**: APPROVED by the human on 31 Oct 2026 (log the install in docs/INSTALLED.md when added).

**Status**: Approved, pending implementation. The slice 2.1 state machine and filter logic are
fully implemented and tested without this dependency. The fallback shortcut
registration code will be written in the next sub-task once approved.

**Raised by**: antigravity-builder-2.1 on 2026-10-31
