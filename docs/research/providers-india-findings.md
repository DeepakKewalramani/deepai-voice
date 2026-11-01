# Provider findings for India: Hindi, Hinglish and other Indian languages (R.10)

Date: 2026-11-01. Read-only web research. No paid API was called, nothing was installed.
Builds on R.4 (`docs/research/providers-findings.md`). Sarvam, Deepgram, ElevenLabs Scribe, AssemblyAI, OpenAI and Groq are not repeated; section 5 lists only additions and corrections for them.
Labels: FACT (seen on an official page or API reference on 1 Nov 2026), INFERENCE (reasoned, not confirmed), UNKNOWN. Pages were read through a page summarizer; re-check prices and field names before building or budgeting.

## 1. Gnani.ai (main question)

### Product names
- FACT: the speech-to-text model is "Gnani Prisma" (v2.5 default, v3.0 latest). The text-to-speech model is "Gnani Timbre" (v2.5, v3.0). Source: https://gnani.ai/ and https://docs.gnani.ai/api/STT/speech-to-text.md
- FACT: "Vachana" is the API platform name (host `api.vachana.ai`, Python SDK `gnani-vachana`) and the launch name "Vachana STT" (Gnani blog, dated 3 Jul 2026). Source: https://gnani.ai/resources/blogs/gnani-ai-launches-vachana-stt-a-foundational-indic-speech-to-text-model-trained-on-one-million-hours-under-the-indiaai-mission
- FACT: "Inya" is not a speech API name. `inya.ai/pricing` redirects (301) to `gnani.ai`, and the homepage does not use the word Inya. INFERENCE: Inya was Gnani's agent-platform brand. Source: https://inya.ai/pricing (redirect seen)

### Speech to text (Prisma)
| Item | Value | Label | Source |
|---|---|---|---|
| REST | `POST https://api.vachana.ai/stt/v3`, `multipart/form-data`, fields `audio_file`, `language_code`, optional `model=prisma-v3.0`, `format` | FACT | https://docs.gnani.ai/api/STT/speech-to-text.md |
| Auth | header `X-API-Key-ID: <key>` (REST and WebSocket upgrade; no query-param option) | FACT | same, and https://docs.gnani.ai/api/STT/stt-websocket.md |
| REST limit | 30 s of audio per request | FACT | https://docs.gnani.ai/api/STT/limits.md |
| Streaming | `wss://api.vachana.ai/stt/v3/stream`, config in upgrade headers (`language_code`, `model`, `x-sample-rate`), raw PCM s16le mono, 1,024-byte frames at real-time pace, JSON `transcript` events with `is_final`, `{"type":"commit"}` to close an utterance; 15 min session cap | FACT | https://docs.gnani.ai/api/STT/stt-websocket.md, limits page |
| Doc conflict | quick-start shows the WS URL without `/stream` | FACT (conflict) | https://docs.gnani.ai/api/introduction/quick-start.md |
| Batch | async jobs, up to 4 h per file | FACT | limits page |
| Languages | `bn-IN en-IN gu-IN hi-IN kn-IN ml-IN mr-IN pa-IN ta-IN te-IN` (10). v3.0 also `indic-auto` or up to 4 codes like `hi-IN,en-IN` | FACT | speech-to-text page |
| Hinglish | `en-IN` accepts English-Hindi mixed audio (Latin output). v3.0 `format=codemix` keeps English words in Latin script. Marketing page claims Hinglish, Tanglish, Benglish | FACT (docs); claim of 96% accuracy is marketing | speech-to-text page, https://www.gnani.ai/speech-to-text-api |
| Response | JSON `{success, request_id, transcript, model, detected_language_code, ...}` | FACT | speech-to-text page |
| OpenAI-compatible | No. Path, auth header and field names differ (`audio_file`, `language_code`, `X-API-Key-ID`) | FACT (from the shapes above) | speech-to-text page |

### Text to speech (Timbre)
| Item | Value | Label | Source |
|---|---|---|---|
| REST | `POST https://api.vachana.ai/api/v1/tts/inference`, JSON `{text, model: "timbre-v2.5"/"timbre-v3.0", voice, language, speed, audio_config}`, returns binary audio (wav, raw, mp3, ogg, mulaw, alaw) | FACT | https://docs.gnani.ai/api/TTS/tts-inference.md |
| Streaming | SSE and WebSocket (`wss://api.vachana.ai/api/v1/tts`) | FACT | tts-inference page, quick-start |
| Languages | Hindi, English, Tamil, Telugu, Kannada, Malayalam, Marathi, Punjabi, Bengali, Gujarati, plus `hi-en` (Hinglish) and `auto` | FACT | tts-inference page |
| Char limit | not given (too long returns 500) | UNKNOWN | tts-inference page |

### Access, price, data
- Self-serve key: conflicting. Docs quick-start says "sign up on the Gnani API platform to generate API keys" and the site links "Get API Key" to `https://app.gnani.ai/voice` (FACT). The launch blog says "available immediately via API access for enterprise customers" and points to "book a demo or request API access" (FACT). The sign-up page did not render for us. Status: UNKNOWN until someone signs up.
- Price: not on any official page we found. UNKNOWN. A third-party listing (callmissed.com) shows Rs 27 per hour for Prisma v2.5; its own review contradicts it. Do not use. Source: https://www.callmissed.com/models/gnani-prisma-v2-5
- Free usage: launch blog says early adopters get 100,000 free minutes, "valid till Jan 12" (year unclear). FACT that it was stated; current status UNKNOWN.
- Rate limits: shown per account at app.gnani.ai, numbers not published. FACT. Source: limits page.
- Data retention or training on customer audio: not stated. UNKNOWN. On-prem and air-gapped deployment offered (FACT, https://www.gnani.ai/speech-to-text-api).

## 2. Other Indian-language providers

### Reverie (RevUp)
- STT REST: `POST https://revapi.reverieinc.com/`, headers `REV-API-KEY`, `REV-APP-ID`, `REV-APPNAME: stt_file`, `src_lang: hi`, `domain: generic`; body multipart `audio_file`. FACT. https://docs.reverieinc.com/usage-guides/speech-to-text-file/getting-started.md
- STT streaming: `wss://revapi.reverieinc.com/stream?apikey=..&appid=..&appname=stt_stream&src_lang=hi&domain=generic`. Key goes in the URL query. FACT. https://docs.reverieinc.com/api-reference/speech-to-text-streaming.md
- Languages: 13 Indic codes plus mixed codes `hi_en` (Hinglish), `ta_en`, `te_en`, `ml_en`, `kn_en`, `mr_en`. FACT. https://docs.reverieinc.com/usage-guides/speech-to-text/supported-languages.md
- TTS: `POST https://revapi.reverieinc.com/`, `REV-APPNAME: tts`, header `speaker: hi_female`, JSON `{text}`; returns WAV 22.05 kHz. Streaming TTS not mentioned. FACT. https://docs.reverieinc.com/api-reference/text-to-speech.md
- Self-serve: yes, free developer account on the RevUp portal; key and App ID shown in dashboard. FACT. https://docs.reverieinc.com/authentication.md
- Free tier: 5 h streaming STT, 5 h file STT, 10,000 TTS chars, 1-year expiry. FACT. https://docs.reverieinc.com/analytics.md
- Price per minute: not published; paid tiers via a form. UNKNOWN.
- Data policy: not found. UNKNOWN.
- OpenAI-compatible: No (custom headers, single root path). FACT from the shape.
- Note: the main site `reverieinc.com` served an expired TLS certificate on 1 Nov 2026 (FACT, fetch error). Minor trust signal only.

### Bhashini (Government of India, ULCA / Dhruva)
- Flow: Pipeline Search (optional), Pipeline Config (`POST https://meity-auth.ulcacontrib.org/ulca/apis/v0/model/getModelsPipeline` with headers `userID` and `ulcaApiKey` from "My Profile"), then Pipeline Compute. FACT. https://bhashini.gitbook.io/bhashini-apis and https://bhashini.gitbook.io/bhashini-apis/pipeline-config-call
- Compute host `https://dhruva-api.bhashini.gov.in/services/inference/pipeline` with an inference key returned by the config call: INFERENCE (seen only in community code, e.g. https://huggingface.co/spaces/pvanand/web-server/blob/main/ai4b.py).
- STT and TTS: both exist as pipeline tasks (ASR, TTS) in Hindi and other ISO-639 languages. FACT (task list on the gitbook page). Streaming: UNKNOWN. Hinglish: UNKNOWN.
- Terms: "Usage of these APIs shall be for the purposes of PoC only"; production needs the paid version from the Bhashini team. FACT. https://bhashini.gitbook.io/bhashini-apis
- OpenAI-compatible: No (two-step config then compute). FACT from the flow.
- Verdict: not suitable for a consumer hub listing today (PoC-only terms).

### Smallest.ai (Pulse STT, Lightning TTS)
- STT REST: `POST https://api.smallest.ai/waves/v1/stt/?model=pulse&language=hi`, raw audio body `application/octet-stream`, header `Authorization: Bearer <key>`. FACT. https://docs.smallest.ai/models/documentation/speech-to-text-pulse/quickstart.md
- STT streaming: `wss://api.smallest.ai/waves/v1/stt/live?model=pulse`. FACT. https://docs.smallest.ai/models/model-cards/speech-to-text/pulse
- Hindi and Hinglish: streaming `hi` is "bilingual, mixed-script", `hi-dev` Devanagari only, `north_indic` auto-detect; code-switching within a session documented. Batch `hi`. FACT. Pulse model card.
- STT price: model card says $0.004/min realtime, $0.003/min batch (FACT). Pricing page shows a "Speech to Text Layer ~$0.009/minute" not tied to Pulse by name (FACT, conflict). https://smallest.ai/pricing
- TTS: Lightning v3.1 and v3.1 Pro; Hindi `hi` supported with Hindi voices; sync POST, SSE and WebSocket (`wss://api.smallest.ai/waves/v1/lightning-v3.1/get_speech/stream`). FACT. https://docs.smallest.ai/waves/model-cards/text-to-speech/lightning-v-3-1 . TTS price: "Text to Speech Layer ~$0.09/minute" (FACT, pricing page); per 1k chars UNKNOWN.
- Self-serve: console sign-up, create API key. $10 free credit on pay-as-you-go. FACT. quickstart and pricing page.
- Data: "HIPAA Zero Data Retention" is a $1000/mo add-on (FACT, pricing page). Default retention and training: UNKNOWN.
- OpenAI-compatible STT: No (raw body, query params). FACT from the shape. Their LLM "Electron" is OpenAI-compatible (FACT, https://docs.smallest.ai/models), not relevant to STT.

### Soniox
- Async: upload file, then `POST https://api.soniox.com/v1/transcriptions` JSON `{model: "stt-async-v5", file_id | audio_url, language_hints}`, header `Authorization: Bearer <key>`, poll status. FACT. https://docs.soniox.com/api-reference/stt/transcriptions/create_transcription , https://docs.soniox.com/stt/models
- Streaming: `wss://stt-rt.soniox.com`, model `stt-rt-v5`. FACT. https://docs.soniox.com/stt/api-reference
- Hindi and code-switching: Hindi supported; "detects and transcribes language switches mid-sentence, even when Hindi is mixed with English", no config needed. FACT. https://soniox.com/speech-to-text/hindi
- Price: token-based, about $0.10/h async, $0.12/h realtime. FACT. https://soniox.com/pricing . Free tier: not stated (UNKNOWN).
- Data: real-time audio "never stored", content "never used to train Soniox models", India region available. FACT (Hindi product page).
- TTS: none found. OpenAI-compatible: No (two-step upload then job). FACT from the shape.

### Speechmatics
- Hindi `hi` listed; no Hindi-English bilingual pack (packs exist for Tamil-English, Mandarin-English etc.). FACT. https://docs.speechmatics.com/speech-to-text/languages
- Price: Standard $0.45/h, Enhanced $0.75/h batch, $0.80/h realtime; $100 free credit, no card. TTS $0.011 per 1k chars (Hindi TTS UNKNOWN). FACT. https://www.speechmatics.com/pricing
- Batch: multipart `POST .../v2/jobs` with `data_file` + `config`, Bearer auth. INFERENCE (from search summary of official docs, not read directly): https://docs.speechmatics.com/batch.yaml . Realtime WebSocket host UNKNOWN.
- OpenAI-compatible: No (job-based). INFERENCE.

### Azure Speech (one line)
hi-IN STT and many Hindi neural TTS voices (e.g. `hi-IN-SwaraNeural`), free F0 tier 5 audio h/month STT and 0.5M chars/month TTS; paid prices did not render on the page (UNKNOWN); not OpenAI-compatible; needs region + key. FACT. https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-support , https://azure.microsoft.com/en-us/pricing/details/speech/

### Google Cloud Speech (one line)
Already in R.4 (Chirp 3, hi-IN GA). Nothing new checked. GCP service-account auth makes it a poor fit for a paste-a-key hub (INFERENCE).

### Other self-serve options found
- Mistral Voxtral: `POST https://api.mistral.ai/v1/audio/transcriptions`, Bearer, fields `file`, `model` (`voxtral-mini-latest`), `language`; Hindi is one of 13 languages; realtime model and SSE streaming exist. FACT. https://docs.mistral.ai/api/endpoint/audio/transcriptions , https://docs.mistral.ai/capabilities/audio_transcription . Per-minute price exists with a free tier, number not shown (UNKNOWN). https://mistral.ai/pricing . OpenAI-compatible: INFERENCE yes for the basic call (same path, Bearer, `file`/`model`/`language`); doc does not show `response_format` or `prompt`. Test before listing.
- Gladia: `POST https://api.gladia.io/v2/pre-recorded` JSON `{audio_url, language_config}`, header `x-gladia-key`; Hindi on Solaria-1; `language_config.code_switching: true` (documented for live). From 0.27 EUR/h async (Solaria-1), 0.68 EUR/h realtime, 50 EUR free credit for one year. FACT. https://docs.gladia.io/api-reference/v2/pre-recorded/init , https://docs.gladia.io/chapters/language/supported-languages , https://docs.gladia.io/chapters/language/code-switching , https://www.gladia.io/pricing . Not OpenAI-compatible (FACT from shape).
- Cartesia: Sonic 3.6 TTS supports Hindi and Hindi written in Latin script (Hinglish). FACT. https://docs.cartesia.ai/build-with-cartesia/tts-models/latest . 1 credit per character, free 20K credits/month, overage $38 to $65 per 1M chars by plan; Ink STT $0.39/h on Scale. FACT. https://cartesia.ai/pricing . Ink STT Hindi: UNKNOWN.

## 3. OpenAI-compatible check (can the generic custom adapter call it?)
| Provider | `/audio/transcriptions` shape | Label |
|---|---|---|
| OpenAI | yes (native) | FACT (R.4) |
| Groq | yes, `https://api.groq.com/openai/v1/audio/transcriptions` | FACT, corrects R.4 INFERENCE. https://console.groq.com/docs/speech-to-text |
| Mistral Voxtral | likely, same path and core fields | INFERENCE |
| Gnani, Reverie, Bhashini, Smallest Pulse, Soniox, Gladia, Speechmatics, Azure | no | FACT from documented shapes (Speechmatics INFERENCE) |
| Sarvam, Deepgram, ElevenLabs, AssemblyAI | no OpenAI-shaped STT found in their docs | INFERENCE (not re-checked) |

Takeaway: the generic adapter covers OpenAI, Groq and probably Mistral. Every India-first provider needs its own adapter.

## 4. Additions and corrections to R.4
- Groq transcription path is FACT (was INFERENCE). Prices FACT: `whisper-large-v3-turbo` $0.04/h, `whisper-large-v3` $0.111/h; 25 MB file limit on free tier. https://console.groq.com/docs/speech-to-text
- Sarvam WebSocket host and auth (was UNKNOWN): `wss://api.sarvam.ai/speech-to-text/ws`, header `Api-Subscription-Key`, query `model` (`saaras:v4` default or `saaras:v3`), `language-code`, `mode` (incl. `codemix`), `sample_rate`; audio sent as base64 inside JSON messages; `{"type":"flush"}` to finalise. FACT. https://docs.sarvam.ai/api-reference-docs/speech-to-text/transcribe/ws . The page lists `mode` for both v3 and v4; whether codemix quality is equal on v4 stays UNKNOWN.
- R.4 said Sarvam is the "only provider that documents" Hinglish. Corrected: Gnani (`format=codemix`, `hi-en` TTS), Reverie (`hi_en`), Smallest (`hi` mixed-script), Soniox (auto code-switching with Hindi) and Cartesia TTS (Latin-script Hindi) also document it. FACT, sources above. None publishes Hinglish accuracy numbers we could verify.

## 5. Recommendation: providers to list in the hub (Hindi plus global)
`verified` = key facts (endpoint, auth, Hindi, price or free tier, self-serve key) seen on official pages. `candidate` = something important is UNKNOWN.

| # | Provider | Role | Hindi / Hinglish | Streaming | Status | Why / gap |
|---|---|---|---|---|---|---|
| 1 | Sarvam | STT + TTS | yes / codemix | WS | verified (R.4 + this note) | best documented Hinglish, rupee pricing |
| 2 | Soniox | STT | yes / auto switch | WS | verified | cheapest per hour, no training on audio, India region |
| 3 | Smallest.ai | STT + TTS | yes / mixed-script `hi` | WS | verified | self-serve, cheap; price figures conflict, retention UNKNOWN |
| 4 | Deepgram | STT (global) | yes / `multi` unclear | WS | verified (R.4) | strong global default |
| 5 | ElevenLabs | STT + TTS (global) | yes / UNKNOWN | WS / stream | verified (R.4) | best global TTS option |
| 6 | Groq (generic OpenAI adapter) | STT | Whisper multilingual | batch | verified | covers OpenAI-compatible endpoints cheaply |
| 7 | Reverie | STT + TTS | yes / `hi_en` | WS (STT) | candidate | self-serve and free tier FACT; price UNKNOWN, key in WS URL query |
| 8 | Gnani (Prisma / Timbre) | STT + TTS | yes / codemix, `hi-en` TTS | WS + SSE | candidate | API fully documented; self-serve vs enterprise-only unclear, price and data policy UNKNOWN |

Not recommended now: Bhashini (PoC-only terms), Google and Azure (cloud-account auth, no Hinglish note), Speechmatics (no Hindi-English pack). Watch list: Mistral Voxtral (could ride the generic adapter), Gladia (code-switching, Hindi), Cartesia (Hinglish TTS).

## 6. Open items
- Gnani: sign up at app.gnani.ai to confirm self-serve keys, price and data terms (free, no paid call needed).
- Reverie and Gnani price per minute.
- Mistral `/audio/transcriptions` with an OpenAI client: one free-tier test, needs approval.
- Hinglish accuracy per provider on our own clips (R.4 Q15, paid, approval needed).
