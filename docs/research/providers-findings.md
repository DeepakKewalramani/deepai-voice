# Provider findings: STT, LLM, TTS APIs and Rust building blocks

Date: 2026-10-31. Detail and source URLs: `.agents/tasks/20261031-voice-dictation/research/15-stt-apis.md`, `16-llm-apis.md`, `17-tts-apis.md`, `18-rust-provider-crates.md`. Independent re-check: `19-provider-validation.md` (24 claims; corrections applied below). Not re-checked: ElevenLabs, AssemblyAI, Google, OpenRouter, Together, Mistral rows and Tauri plugin facts; treat them as single-source.
Labels: FACT (seen on the official page on 31 Oct 2026), INFERENCE, UNKNOWN. No paid API was called. Prices came through a page summarizer in places; re-check before budgeting.

## Constraint (USER-PROVIDED, 31 Oct 2026)
API keys only. Claude Team/Pro and Google Antigravity subscriptions are not API access. The app does not reuse subscription logins.

## 1. Speech to text
| Provider | Batch / streaming | Hindi | Hinglish (code-mixed) | Price (listed) | Notes |
|---|---|---|---|---|---|
| Local whisper.cpp | batch | multilingual models only (not `.en`) | UNKNOWN | free | already in ADR 0001; `base.en` is English only |
| Sarvam (`saaras:v3` for codemix; `saaras:v4` default) | REST under 30 s (documented guidance, not a stated hard max; enforce on our side); Batch API for longer; WebSocket `/speech-to-text/ws` | FACT, hi-IN + 22 Indic | FACT: `mode=codemix` on `saaras:v3` (English words in Latin, Hindi in Devanagari). Docs contradict on v4 + codemix: UNKNOWN (19). Only provider that documents it | Rs 0.49/min | auth `api-subscription-key` (REST). WS host and auth UNKNOWN |
| Deepgram (`nova-3`) | both | FACT `hi` on nova-3 | `language=multi` exists; Hindi inside it UNKNOWN | $0.0052/min pre-recorded, $0.0092/min multilingual stream | auth `Authorization: Token <key>` (FACT, 19) |
| ElevenLabs Scribe v2 | both (`scribe_v2_realtime`) | FACT, high-accuracy tier | UNKNOWN | $0.22/hr batch, $0.39/hr realtime | silence billed; ~150 ms realtime claim, footnote unreadable |
| AssemblyAI | both | FACT, "good" tier | `code_switching`, needs `en` as one language | $0.15 to $0.21/hr batch | idle stream time billed |
| OpenAI (`gpt-transcribe`) | batch; realtime model separate | not stated on page | UNKNOWN | $0.0045/min | whisper-1 and gpt-4o-transcribe shut down 2027-02-26 (FACT, 15) |
| Groq (`whisper-large-v3-turbo`) | batch | INFERENCE (Whisper is multilingual) | UNKNOWN | $0.04/hr | base `https://api.groq.com/openai/v1` FACT; `/audio/transcriptions` path INFERENCE |
| Google STT V2 (`chirp_3`) | both | FACT hi-IN GA | not stated | ~$0.016/min (from search summary) | GCP auth, not a simple key (INFERENCE) |

Takeaways:
- Sarvam is the only documented Hinglish fit. Its REST limit is under 30 s, while our `maxRecordSeconds` is 120. Clips over 30 s need its Batch API, chunking, or the WebSocket (Q16).
- No provider publishes Hinglish accuracy numbers. A small bench on our own recordings is needed before choosing a default (Q15, paid calls, needs approval).
- Local `base.en` cannot do Hindi. Local Hindi needs a multilingual model (`base` or `small`), quality UNKNOWN.

## 2. LLM (cleanup)
| Provider | Endpoint | Auth | Fast model and price (in / out per MTok) | Notes |
|---|---|---|---|---|
| Anthropic | `POST /v1/messages` | `x-api-key` + `anthropic-version: 2023-06-01` | `claude-haiku-5-5`, $0.10 / $0.50 | no prefill (400), omit temperature, adaptive thinking on by default: use `output_config.effort: "low"` and leave room in `max_tokens`; tokenizer ~30% more tokens than Haiku 4.5 |
| OpenAI | `/v1/chat/completions`, `/v1/responses` | Bearer | gpt-5-nano $0.05 / $0.40; gpt-4.1-nano $0.10 / $0.40 | |
| Gemini | OpenAI-compat `https://generativelanguage.googleapis.com/v1beta/openai/` | Bearer | gemini-2.5-flash-lite $0.10 / $0.40 | works through the generic adapter |
| OpenRouter | `https://openrouter.ai/api/v1` | Bearer | many, ids like `openai/gpt-5.2` | generic adapter |
| Groq | `https://api.groq.com/openai/v1` | Bearer | prices conflict between sources | rejects `logprobs`, `logit_bias`, `messages[].name`; `n` must be 1 |
| Together | `https://api.together.ai/v1` | Bearer | own model ids | generic adapter |
| Ollama (local) | native `/api/chat` (NDJSON), `/v1` OpenAI-compat | none (dummy key) | free | `keep_alive` avoids reload; data stays on device |
| DeepSeek, Mistral | OpenAI-compat (INFERENCE) | Bearer | weak sources only | verify before listing |

Takeaways:
- Two adapters cover all of the above: native Anthropic plus one generic OpenAI-compatible (`/chat/completions`, SSE). Ollama works through `/v1`.
- The generic adapter must send only standard fields (no `logprobs`, no `name`, `n=1`) to stay compatible with Groq and others.
- Hindi cleanup quality is UNKNOWN for every model; part of the Q15 bench.

## 3. Text to speech (v3)
| Provider | Hindi | Streaming | Price | Notes |
|---|---|---|---|---|
| Sarvam Bulbul v4-flash / v3 | FACT hi-IN, code-mixed | WebSocket (MP3) | Rs 1 / Rs 3 per 1k chars | cheapest with Hindi |
| ElevenLabs Flash v2.5, v3, Multilingual v2 | FACT (not on flash_v2) | `/stream` | $0.04 / $0.08 per 1k chars | ~100 ms median claimed (Flash v2.5) |
| Deepgram Aura-2 | NOT listed | WebSocket | $0.030 per 1k chars | English and few EU languages only |
| OpenAI gpt-4o-mini-tts, tts-1, tts-1-hd | yes | yes | tts-1 price not found | all three shut down 6 Jan 2027 (FACT, 17). Do not build on them |
| macOS built-in | FACT: Hindi voice Lekha installed on this Mac (`say -v ?`) | local | free | offline fallback |
| Windows WinRT SpeechSynthesizer | needs voice pack, UNKNOWN | local | free | later |

## 4. Rust building blocks
- Keys: `keyring` 4.2.0 (MSRV 1.88), store crates per OS. Call it from Rust commands. tauri-plugin-stronghold is not recommended by Tauri maintainers (2023) and has its own master password; skip it.
- HTTP: `reqwest` 0.13.5, rustls default, HTTP/2 default; enable `json`, `multipart`, `stream` features.
- Streaming: `tokio-tungstenite` 0.30 for STT sockets; `eventsource-stream` over `bytes_stream` for SSE. Avoid `reqwest-eventsource` (auto-retries POST).
- Multi-provider crates: `genai` 0.7.x is LLM-only, pre-1.0. No mature multi-provider STT/TTS crate. Write our own traits (ADR 0002).
- Webview: only `set_key`, `delete_key`, `has_key` commands touch keys; restrict commands per window in `build.rs`; do not grant `store:default` or the JS http plugin.
- Toolchain: plugins need Rust 1.95+; installed 1.99.1 (FACT, ADR 0001).

## 5. Unknowns that need a spike or a decision
- Hinglish accuracy and latency per STT and LLM (Q15, paid, approval needed).
- Sarvam long-clip path (Q16).
- Keychain prompts after a re-sign with `keyring` 4 (spike S6).
- aws-lc build on Windows (W0).
