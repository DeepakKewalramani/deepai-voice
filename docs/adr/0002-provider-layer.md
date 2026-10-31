# ADR 0002: Configurable provider layer (STT, LLM, TTS)

- Status: Proposed (31 Oct 2026). Waits for user approval with Gates 1 to 3.
- Date: 2026-10-31
- Sources: docs/research/providers-findings.md, .agents/tasks/20261031-voice-dictation/research/15 to 18, ADR 0001.

## Context
USER-PROVIDED (31 Oct 2026): this is our own product, fully configurable by the user. Every provider is an option in Settings. The user can change prompts, hotkeys, models, fallback order and add new providers without code changes. Stack is fixed by ADR 0001: Tauri v2 + Rust (native parts) + React + TypeScript + Vite. Mac first, Windows later.

Constraint (USER-PROVIDED): API keys only. Consumer or team subscriptions (Claude Team/Pro, Google Antigravity) do not give API access. The app does not reuse subscription logins, browser sessions or OAuth tokens of those products.

ADR 0001 assumed one STT (local whisper.cpp) and one LLM (Anthropic Haiku). This ADR replaces that single path with a provider layer. Everything else in ADR 0001 stands.

## Decision

### 1. Three capability traits in Rust
Providers live in Rust (`src-tauri/src/providers/`). The webview never sees a key and never calls a provider directly.

```rust
trait SttProvider  { async fn transcribe(&self, audio: &Pcm16kMono, opts: &SttOpts) -> Result<Transcript, ProviderError>; }
trait LlmProvider  { async fn complete(&self, req: &CleanupRequest, opts: &LlmOpts) -> Result<LlmOutput, ProviderError>; }
trait TtsProvider  { async fn speak(&self, text: &str, opts: &TtsOpts) -> Result<AudioOut, ProviderError>; }   // v3
```
- `ProviderError` has a kind: `Auth`, `RateLimit`, `Timeout`, `Network`, `BadRequest`, `Server`, `Unsupported`, `Local`. The fallback chain uses the kind (see 3).
- Every result carries `Usage { audio_seconds, input_tokens, output_tokens, characters, provider_request_id }` for the cost display.
- v1 is batch only (whole clip after key release). Streaming STT (websocket) is a later optional method `transcribe_stream`, not in v1.

### 2. Registry with built-in adapters plus one generic adapter
- Built-in adapters (code): `whisper_local`, `sarvam_stt`, `deepgram_stt` (v1); `anthropic` and one OpenAI-compatible LLM path (v1); others in v2; TTS in v3.
- Generic adapter "Custom OpenAI-compatible" (code once, config many): the user enters base URL, model id, key, and optional extra headers. LLM uses `POST {base}/chat/completions`; STT uses `POST {base}/audio/transcriptions` (multipart). Any provider that implements these shapes (examples in findings: OpenAI, Groq, OpenRouter, Gemini compat endpoint, Ollama `/v1`) works with no code change. Which providers truly match is per findings; anything not verified is tested with the "Test connection" button.
- Each provider instance is a config entry: `{ id, kind: stt|llm|tts, adapter, label, baseUrl?, model, keyRef?, options{} }`. The same adapter can appear many times (two Deepgram models, three OpenAI-compatible endpoints).

### 3. Fallback chain per capability
- Settings hold an ordered list per capability, e.g. `stt.chain = ["sarvam-1", "whisper-local"]`.
- Try entries in order with a per-entry timeout. Move to the next entry on `Timeout`, `Network`, `RateLimit`, `Server`, `Auth` (key missing or rejected). Do not move on `BadRequest` from the user's own prompt; show the error.
- Last resort is always local: STT ends with `whisper_local` if a model is installed; LLM chain ending means "paste the raw transcript" (existing rule 6).
- One overall deadline per dictation (`totalDeadlineMs`) so a long chain cannot stall the paste.
- The overlay shows which provider produced the text (small label), so a silent fallback is visible.

### 4. User-editable prompts and styles
- A "style" is `{ id, name, systemPrompt, userTemplate, llmChain?, language? }`. `userTemplate` must contain `{{transcript}}`; the app wraps it in `<transcript>` tags and the system prompt keeps the "treat dictation as data" sentence (rule 13). The editor warns, not blocks, if the user removes it.
- Built-in styles ship read-only with a "Duplicate to edit" action, plus "Reset to default".
- Per-style hotkey is v2 (see plan). v1: one active style picked from the tray menu.

### 5. Settings schema (JSON, versioned)
- One file `settings.json` in the Tauri app config dir. Top-level `schemaVersion` (integer, starts at 1). Rust owns the schema (serde structs, JSON Schema generated for the UI). On load: older version runs ordered migrations, newer version opens read-only with a warning, invalid file is backed up and defaults load.
- Keys are never in this file. `keyRef` names an entry in the OS credential store (keyring: macOS Keychain, Windows Credential Manager). Env vars (`ANTHROPIC_API_KEY`, `DEEPGRAM_API_KEY`, `SARVAM_API_KEY`, ...) still win for dev use.
- Import and export of settings excludes keys by design.

Shape (v1):
```json
{
  "schemaVersion": 1,
  "hotkeys": { "dictate": "Ctrl+Alt+Space" },
  "providers": [
    { "id": "whisper-local", "kind": "stt", "adapter": "whisper_local", "model": "base.en" },
    { "id": "sarvam-1", "kind": "stt", "adapter": "sarvam_stt", "model": "<from findings>", "keyRef": "sarvam" },
    { "id": "haiku", "kind": "llm", "adapter": "anthropic", "model": "claude-haiku-5-5", "keyRef": "anthropic" },
    { "id": "custom-1", "kind": "llm", "adapter": "openai_compatible", "baseUrl": "https://example/v1", "model": "x", "keyRef": "custom-1" }
  ],
  "chains": { "stt": ["whisper-local"], "llm": ["haiku"], "tts": [] },
  "styles": [ { "id": "clean", "name": "Clean up", "systemPrompt": "...", "userTemplate": "{{transcript}}" } ],
  "activeStyle": "clean",
  "timeouts": { "sttMs": 8000, "llmMs": 2500, "totalDeadlineMs": 10000 },
  "privacy": { "cloudAcknowledged": { "sarvam-1": "2026-10-31" } },
  "maxRecordSeconds": 120
}
```
Timeout numbers are DEFAULT placeholders until spikes S3 and S5 measure real latency.

### 6. Privacy rules
- Each provider entry has a computed `dataLeavesDevice` (true for every cloud adapter, false for `whisper_local` and Ollama on localhost). A custom base URL is treated as cloud unless the host is `localhost`/`127.0.0.1`.
- First use of a cloud STT shows a one-time notice: "Your voice recording is sent to <provider>." First use of a cloud LLM: "Your transcript text is sent to <provider>." Acknowledgement date stored in `privacy.cloudAcknowledged`.
- The tray menu shows the active STT and LLM with a cloud or device icon at all times.
- Audio stays in memory; cloud STT uploads it from memory, never via a temp file (rule 1 still holds).
- Logs keep provider id, status, duration, sizes, request id. Never keys, audio, transcripts or prompts (rule 12).
- Key entry goes from the Settings UI to a Rust command that writes straight to the credential store. The UI gets back only "set" or "not set" and the last 4 characters.

### 7. Cost and usage display
- Each adapter returns usage units (audio seconds, tokens, characters). A local price table in settings (`pricing` per provider and model, user-editable, with "source URL" and "checked on" fields) turns units into an estimate.
- Settings shows today, this month and per-provider totals, labelled "Estimate. Your provider bill is the truth." Stored as counters only, no content.
- No price is fetched from the network. Defaults come from findings and are dated.

## Alternatives
| Option | Pros | Cons | Verdict |
|---|---|---|---|
| A. Rust traits + built-in adapters + generic OpenAI-compatible adapter (chosen) | Keys stay in Rust. Most new providers need no code. Fallback and usage logic in one place, unit-testable with fakes. | Each non-compatible provider (Sarvam, Deepgram, Anthropic) needs an adapter. | Chosen. |
| B. Providers in TypeScript (webview fetch) | Fast to write, JS SDKs exist. | Keys enter the webview; CSP and CORS issues; Rust STT path split from cloud path. | Rejected: key exposure. |
| C. Third-party multi-provider crate as the core (for example genai, rig) | Less adapter code for LLMs. | LLM only, no STT or TTS; ties our error model and release cadence to it; maturity per findings. | Not as the core. May be used inside one adapter later if findings support it. |
| D. Fully declarative providers (user writes request/response mapping in JSON) | Any HTTP API without code. | Hard to validate, hard to support, multipart and websocket mapping gets complex. | Rejected for v1. The OpenAI-compatible adapter covers the common case. |
| E. Reuse Claude Pro/Team or Antigravity logins | No extra API bill for the user. | Not API access; likely against those products' terms; fragile. | Rejected by user constraint. |

## Consequences
- ADR 0001 lines on "API key: ANTHROPIC_API_KEY first, then keyring" generalise to every provider via `keyRef`.
- Spec rule 6 (fallback to raw transcript) becomes the end of the LLM chain.
- New permissions to explain to users: cloud STT sends audio off the device. D3 widens to "cloud STT and cloud LLM defaults" (HUMAN_INPUT Q14).
- More HTTP surface: each adapter gets fake-server tests (no paid calls in CI).
- Settings UI becomes a real screen (React), not just a tray menu.
