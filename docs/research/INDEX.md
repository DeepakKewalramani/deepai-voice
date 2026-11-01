# Research register (read this before you research anything)

One row per research note, spike or investigation. Every AI, account and tool uses this file.
Rule (full text in `AGENTS.md` section 7): read this index and the top of `docs/tasks/INBOX.md` before you start, post a `RESEARCH:` line in `INBOX.md`, and add your row here when you finish. If a topic is already answered below, reuse the answer and say so in your INBOX line.

Labels: FACT (seen on an official page, paper, command output or in our repo, with a source), INFERENCE (reasoned, not confirmed), UNKNOWN. Status: DONE, STOPPED, IN PROGRESS.

| ID | Topic | Date | Owner | Status | Note (where the answer is) | Verdict in one line |
|---|---|---|---|---|---|---|
| R.1 | Voice dictation: validated findings (audio, Whisper, paste, hotkey) | 2026-10-31 | validator pass | DONE | `docs/research/voice-dictation-findings.md` | Source of the validated FACT list for dictation. |
| R.2 | Voice dictation: end-to-end user flow check | 2026-10-31 | planning job | DONE | `docs/research/voice-dictation-flow.md` | Spikes S1 onward are listed there, nothing was installed. |
| R.3 | Cross-platform (macOS first, Windows later) | 2026-10-31 | planning job | DONE | `docs/research/cross-platform-findings.md` | Keep native parts behind traits so Windows can follow. |
| R.4 | Providers: speech, language model and voice APIs, Rust building blocks | 2026-10-31 | planning job | DONE | `docs/research/providers-findings.md` | No paid API was called. Re-check prices before budgeting. |
| R.5 | Meeting notes, live translator, shortcuts, install | 2026-10-31 | planning job | DONE | `docs/research/meetings-translator-findings.md` | Input for phases 3 and 4. |
| R.6 | Streaming engine options for instant typing: sherpa-onnx, Moonshine, Vosk, Parakeet | 2026-11-01 | claude main + research subagent | DONE | Summary below; full comparison in `docs/specs/language-independent-speech.md` section 5.3 | sherpa-onnx is the only option with an official Rust crate and a real streaming API. No Hindi model in its official list. |
| R.7 | sherpa-onnx trial on this Intel Mac (throwaway project outside the repo, approved crate and model) | 2026-11-01 | claude main + subagent | DONE | Summary below. Scratch files in `~/.cache/deepai-spike/` on the machine that ran it, not in the repo. | Viable but weak: see numbers below. Not built into the app. |
| R.8 | Apple on-device speech: which locales run on device on this Mac | 2026-11-01 | claude main | DONE | `docs/specs/language-independent-speech.md` section 2 | en-US and en-IN run on device. en-GB and hi-IN need a server, so they are not usable offline. |
| R.9 | Language-independent speech engine: what is needed, how, and the own-model path | 2026-11-01 | claude main + research subagent | DONE | `docs/specs/language-independent-speech.md` | Do not train first. Multilingual Whisper, then an Indian-language engine, then Hinglish fine-tuning, own model only if measurements show a gap. |
| R.10 | Providers for India: Gnani, Reverie, Bhashini, Smallest, Soniox, Speechmatics and others for Hindi and Hinglish | 2026-11-01 | research subagent | DONE | `docs/research/providers-india-findings.md` | List Sarvam, Soniox, Smallest, Deepgram, ElevenLabs, Groq as verified; Reverie and Gnani as candidates (price or access unknown). |

## Numbers kept from R.7 (FACT, measured on 1 Nov 2026, Intel i9, CPU only)

- Crate `sherpa-onnx` 2.0.1 built on the first try in 2 min 27 s. Its build downloads one native archive (20.4 MB) from the project's GitHub release.
- Model `sherpa-onnx-streaming-zipformer-en-2023-06-26` int8 files, 72.7 MB total.
- Speed: 0.21 to 0.31 times real time. First words appear 0.8 to 1.2 s after the start of speech. Model load 9.4 to 11.0 s.
- Accuracy on our five test recordings: word error 9% to 36%. On a harder sentence with Indian place names: 45%. Output is uppercase with no punctuation.
- Without silence padding before the audio, the first word was dropped.

## Open questions nobody has answered yet (claim one on INBOX.md before you start)

- Speed of Whisper `small`, `medium` and `large-v3-turbo` on this Mac (not measured).
- Whether the IndicConformer model can stream.
- Hindi on-device support in Apple's newer speech API on macOS 26.
- Licence of the MUCS Hindi-English dataset and the current Common Voice Hindi hours.
- Real accuracy of every engine on our own test set of Indian-English and Hindi voices (not built yet).
