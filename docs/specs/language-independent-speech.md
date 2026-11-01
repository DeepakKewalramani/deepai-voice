# Language-independent speech engine: what is needed, how, and the own-model path

Status: DRAFT for your decision, 1 Nov 2026. Research done 1 Nov 2026 with sources.
Labels used everywhere: **[F]** fact (seen on an official page, paper or in our repo, with link), **[I]** inference (reasoned, not confirmed), **[U]** unknown (could not verify).

---

## 1. Summary (read this first)

**Right now,** English dictation works well on this Mac. It runs fully offline. Hindi, Hinglish and other Indian languages do not work, because both engines we use today are English only: Whisper `base.en` and Apple's recognizer set to Indian English.

**The fix is not one switch.** "Language independent" means five things:

1. A recognizer that knows many languages (a multilingual model).
2. Picking the language: a setting, or automatic detection.
3. Mixed Hindi and English in one sentence (Hinglish), which almost no ready-made model does well.
4. Typing non-English letters into any app correctly.
5. Language-specific cleanup (our noise and filler filters are English today).

**Recommendation (details in section 8):**

- **Do not build our own model first.** Use proven open models, measure on real voices, and only train our own if the measurements show a gap.
- **Step 1:** add language choice and a multilingual Whisper (`small`, 466 MB, or a quantized `large-v3-turbo`, 547 MB). Keep Apple's streaming for English.
- **Step 2:** add an Indian-language engine. The best candidate with a permissive licence is the AI4Bharat family (IndicWhisper for Hindi, IndicConformer for 22 languages).
- **Step 3:** Hinglish needs fine-tuning on code-mixed data. This is the first place where "own model" really starts.
- **Step 4:** a fully own model, only if steps 1 to 3 fall short of the accuracy target.

**Decisions I need from you** are in section 11.

---

## 2. Where we are today (facts from our repo)

| Part | State | Source |
|---|---|---|
| Streaming recognizer | Apple on-device speech, locale Indian English first, US English as fallback. Works offline. | `src-tauri/src/dictation/engine.rs` [F] |
| Final-text recognizer | Whisper `base.en`, English only. Language is hard-coded to `en`. | `src-tauri/src/stt/local_whisper/mod.rs` (`params.set_language(Some("en"))`) [F] |
| Model file | `ggml-base.en.bin`, 148 MB, in the app's data folder | `src-tauri/src/dictation/mod.rs` [F] |
| Hallucination and filler filters | English phrases only ("thank you", "um", "uh") | `src-tauri/src/stt/filters/` [F] |
| Engine interfaces | `SttProvider` (batch), `StreamingStt` (streaming), `ProviderChain` with fallback | `src-tauri/src/stt/` [F] |
| Apple on-device languages on this Mac | `en-US` and `en-IN` on device. `en-GB` and `hi-IN` need a server, so they are not usable offline. | Checked on this Mac on 1 Nov 2026 with `supportsOnDeviceRecognition` [F] |
| Speed on this Mac (Intel i9) | `base.en`: about 0.8 s per pass on an idle Mac, 1.5 to 3 s when the CPU is busy | Our logs [F] |

**Good news:** the engine interfaces already exist. Adding a language engine is an adapter, not a rewrite.

---

## 3. What "language independent" means, piece by piece

| Piece | What it is | Example | Hard? |
|---|---|---|---|
| A. Multilingual recognizer | One model that understands many languages | Hindi speech becomes Devanagari text | Medium |
| B. Language choice | You pick the language, or the app detects it | Setting "Hindi", or auto | Easy to medium |
| C. Code-switching | Two languages in one sentence | "Kal meeting hai, please send the report" | **Hard** |
| D. Typing other scripts | Putting Devanagari, Tamil and others at the cursor | Typing "नमस्ते" in Notes | Medium |
| E. Language-aware cleanup | Filters, Dictionary and later punctuation per language | Hindi filler words, names in Devanagari | Medium |

Piece C is the hard one. Language detection per utterance cannot split a sentence in the middle, so Hinglish needs a model trained on mixed data. [I]

---

## 4. Requirements checklist (please confirm or change)

| Requirement | Proposed default |
|---|---|
| Languages, first wave | English, Hindi, Hinglish |
| Languages, second wave | Marathi, Tamil, Telugu, Bengali, Gujarati, Kannada, Malayalam, Punjabi, Urdu |
| Offline and private | Yes. Audio never leaves the Mac. |
| Instant typing (words as you speak) | Required for English. For other languages: best effort, then corrected per sentence. |
| Accuracy target | To be set per language. A starting point is a word error rate under 15% for Hindi and under 20% for Hinglish on our own test set. |
| Mac hardware | Intel Macs today (this one is an i9). Apple silicon and Windows later. |
| App size | Models are downloaded on first use, not bundled, so the app stays small. |
| Cost | Free open models first. No paid cloud. |

---

## 5. The options, with facts

### 5.1 Whisper multilingual (what we already run, in a bigger and multilingual form)

- **Models and sizes (ggml files)** [F]: tiny 75 MiB, base 142 MiB, small 466 MiB, medium 1.5 GiB, large-v3 2.9 GiB, large-v3-turbo 1.5 GiB. Quantized: large-v3 q5_0 1.1 GiB, large-v3-turbo q5_0 547 MiB. Source: whisper.cpp model list https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md
- **Parameters and licence** [F]: tiny 39M, base 74M, small 244M, medium 769M, large 1550M, Apache-2.0. large-v3-turbo has 809M parameters, MIT licence, and a much shorter decoder, with "minor quality degradation". Sources: https://huggingface.co/openai/whisper-large-v3 and https://huggingface.co/openai/whisper-large-v3-turbo
- **Languages** [F]: 99 listed, but the model card says it is strong in only about 10. Accuracy follows how much training data a language had.
- **Hindi accuracy (word error rate on FLEURS, lower is better)** [F]: tiny 102.3, base 101.1, small 38.4, medium 26.8, large 26.9, large-v2 21.5. Source: the Whisper paper, Appendix D, https://arxiv.org/abs/2212.04356
- **Other Indian languages (large-v2)** [F]: Marathi 38.3, Urdu 22.6, Tamil 17.5, Bengali 104.1, Telugu 99.0. Bengali and Telugu are effectively unusable. Same paper.
- **Real Indian audio is harder** [F]: one vendor reports large-v3 at 61.9% word error on Common Voice Hindi (vendor's own figure). https://huggingface.co/Oriserve/Whisper-Hindi2Hinglish-Prime
- **Reading of the numbers** [I]: a multilingual `base` is useless for Hindi. `small` is the minimum. `medium` or `turbo` is realistic. Hinglish: no official statement found [U].
- **Features we need** [F]: whisper.cpp supports automatic language detection (`-l auto`, detect-only `-dl`), an initial prompt for custom words (`--prompt`) and carrying that prompt (`--carry-initial-prompt`). https://github.com/ggml-org/whisper.cpp/blob/master/examples/cli/README.md
- **Speed** [I]: `small` is roughly 3 times slower than `base` and `medium` about 10 times slower on this Mac. A quantized `turbo` is the best speed per accuracy point, but this is not measured here [U]. We would measure before choosing.

### 5.2 Indian-language models from AI4Bharat

- **IndicConformer 600M multilingual** [F]: 22 Indian languages, MIT licence, 600M parameters, hybrid CTC and RNN-T decoding, ONNX available. The card does not say it streams [U]. The card also points to a newer "Indic-Transcribe" model (25 languages). https://huggingface.co/ai4bharat/indic-conformer-600m-multilingual
- **Per-language NeMo checkpoints** [F]: for example the Hindi one (120M encoder). These repos are gated and licences differ (MIT for Hindi, CC-BY-4.0 seen for Telugu). https://huggingface.co/ai4bharat/indicconformer_stt_hi_hybrid_ctc_rnnt_large
- **IndicWhisper** [F]: Whisper `medium` fine-tuned per language on the Vistaar training set (10,700 hours, 12 languages). MIT licence. Average Hindi word error 13.6, against 20.0 and 23.9 for two large cloud services. Best on 39 of 59 benchmarks. https://github.com/AI4Bharat/vistaar and https://arxiv.org/abs/2305.15386
- **Reading** [I]: this is the strongest verified route for Hindi accuracy with a commercial-friendly licence. The cost is size and speed (a `medium`-size model).

### 5.3 Streaming engines (words as you speak)

- **sherpa-onnx** [F]: its official model list has no Hindi model. The only Indian-language streaming entry is a Bengali Zipformer. It can load NeMo CTC models after export. https://k2-fsa.github.io/sherpa/onnx/pretrained_models/index.html
- **Community Hindi streaming build** [U]: a roughly 174 MB Hindi streaming model exists on Hugging Face, and its author says it is not for Hinglish. Not verified. https://huggingface.co/mobilebytesensei/betterflow-salesken-hindi-streaming
- **NVIDIA Parakeet and Canary** [F]: 25 European languages, no Hindi, CC-BY-4.0. https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3
- **Moonshine** [F]: no Indian languages in the published list. English models are MIT, legacy non-English models are non-commercial. https://github.com/moonshine-ai/moonshine
- **Reading** [I]: there is no ready, proven, permissive streaming model for Hindi. For Hindi we would type per sentence after a pause, not word by word, until we have one.

### 5.4 Apple on-device speech

- **Facts** [F]: macOS Dictation lists "Hindi (India)", but the separate on-device list of 47 entries does not include Hindi. https://www.apple.com/macos/feature-availability/ On this Mac, `hi-IN` reports "not on device" [F].
- **Reading** [I]: Apple's engine stays our English streaming engine. It cannot be our Hindi engine offline today. We should re-check with `supportsOnDeviceRecognition` on newer macOS versions.

### 5.5 Models we cannot use commercially

- **Meta MMS** [F]: 1,162 languages, licence CC-BY-NC-4.0, so no commercial use. https://huggingface.co/facebook/mms-1b-all
- **SeamlessM4T v2** [F]: 101 speech-input languages including Hindi, also CC-BY-NC-4.0. https://huggingface.co/facebook/seamless-m4t-v2-large
- **Rule:** we only ship models whose licence allows commercial use. Check each licence before adding a model.

### 5.6 Comparison, in one table

| Option | Hindi quality | Streaming | Size | Licence | Effort | Verdict |
|---|---|---|---|---|---|---|
| Whisper multilingual `small` | Weak to fair (38.4 error) | No, per sentence | 466 MiB | Apache-2.0 | Low | Step 1 baseline |
| Whisper `large-v3-turbo` (q5_0) | Better [I] | No | 547 MiB | MIT | Low | Step 1 candidate |
| IndicWhisper (medium, fine-tuned) | **Best verified** (13.6) | No | about 1.5 GiB [I] | MIT | Medium (convert to ggml) | Step 2 first choice for Hindi |
| IndicConformer 600M | Strong for 22 languages [I] | Not stated [U] | not stated [U] | MIT | Medium to high | Step 2 for many languages |
| sherpa-onnx streaming | No Hindi model [F] | Yes | small | Apache-2.0 | n/a for Hindi | Not for Hindi yet |
| Apple on-device | Hindi not on device [F] | Yes | none | system | none | English only |
| Meta MMS, Seamless | Wide coverage | No | large | **Non-commercial** | n/a | Do not use |
| Our own model | Whatever we train | Optional | Our choice | Ours | **Very high** | Step 4, only if needed |

---

## 6. Recommended architecture

**Principle:** one place decides which engine handles which language. Engines are plug-ins behind interfaces we already have.

1. **Language setting.** Settings gets a Language choice: Auto, English, Hindi, and more as they are added. It is stored with the other settings.
2. **Engine router.** Extend `dictation/engine.rs`: input is the language and what is installed; output is the engine plus a fixed reason string. Example: English goes to Apple streaming; Hindi goes to the Hindi engine; anything not installed falls back to Whisper multilingual.
3. **Model manager.** A small part that lists models, downloads them on request (with your approval of each model), checks the file hash, stores them in the app data folder, and shows sizes and licences in Settings. This replaces the single hard-coded `ggml-base.en.bin`.
4. **Language detection (Auto).** For the first pass use Whisper's own detection on the first second of audio. A separate small detector is available: the SpeechBrain VoxLingua107 model covers 107 languages including Hindi, 6.7% error on its dev set, Apache-2.0 [F] https://huggingface.co/speechbrain/lang-id-voxlingua107-ecapa . Detection per utterance cannot split a Hinglish sentence [I].
5. **Per-language filters.** Move the English filler and hallucination lists into per-language data files. Start with English and Hindi.
6. **Per-language Dictionary.** The Dictionary slice (names and terms) must carry a language tag, and pass words as hints to Whisper (`--prompt`) and to any engine that supports hints.
7. **Typing other scripts.** See section 7.
8. **Instant typing policy.** English streams word by word. Other languages type after each pause (sentence by sentence) until a streaming model exists for that language. This is honest and still much faster than waiting for key release.

**Why this over the alternatives:**

- **Over "one big model for everything":** the biggest models are slow on this Mac and Hindi still needs a Hindi-tuned model for good accuracy.
- **Over "cloud recognition":** you asked for offline and private. A cloud engine can be a later optional plug-in.
- **Over "train our own now":** training needs data, GPUs and months. Open Hindi models already reach 13.6% word error. We should measure them on your voice before spending on training.

---

## 7. Typing non-Latin scripts (Devanagari and others)

- **How we type today:** synthetic key events carrying Unicode text, in chunks of up to 16 UTF-16 units (`src-tauri/src/paste/typing.rs`) [F].
- **Known limits** [F, community reports, not Apple]: some apps ignore the Unicode text and translate from a virtual keycode; a reported limit of about 20 UTF-16 units per event, usually handled by sending chunks without splitting surrogate pairs and with small delays. https://github.com/enigo-rs/enigo/issues/68
- **Risk** [I]: Devanagari combining marks (matras) could split across chunks. Chunk on grapheme boundaries, not on a fixed count.
- **Backspace correction** [I]: our correction counts visible characters. For Devanagari one visible unit can be several Unicode code points, so the counter must use grapheme clusters, not characters. This needs a test with real Hindi text before we enable any Indian language.
- **First task:** a tiny spike that types a Hindi sentence into TextEdit, Notes, Mail and a browser, then backspaces part of it.

---

## 8. Phased plan (maps to roadmap slices)

| Step | What | Output | Needs from you |
|---|---|---|---|
| **L0** (in progress) | English: streaming in Indian English, Dictionary for names (2.14) | Names like "Deepak" right | Nothing |
| **L1** | Language setting, model manager, Whisper multilingual `small` or turbo (q5_0), per-language filters, typing spike for Devanagari | Hindi works at a basic level, English unchanged | Approve one model download (466 MB or 547 MB) |
| **L2** | Hindi engine: IndicWhisper converted to ggml (via `convert-h5-to-ggml.py`) or IndicConformer through ONNX; evaluation on our own test set | Good Hindi, then more Indian languages | Approve model downloads; confirm each licence |
| **L3** | Hinglish: fine-tune a model on code-mixed data (MUCS about 90 hours plus our own recordings) | Hinglish usable | Recordings from real users with consent; GPU budget |
| **L4** | Own model, only if L1 to L3 miss the target | Our model, our data | A real budget and a small team (section 9) |

**Evaluation method (must be done before we claim anything):**

1. Build a small test set: 100 sentences per language, spoken by 5 or more real people with different accents, plus noisy rooms. Use public test splits (FLEURS, Kathbath) as a second opinion.
2. Measure word error rate (and character error rate for scripts without spaces) for each engine on the same set.
3. Record speed on this Mac: time from end of speech to text.
4. Write the results into `docs/` and choose the engine from the numbers, not from claims.

---

## 9. If we build our own model: what it takes

### 9.1 Four meanings of "own model", from small to huge

| Path | What it is | Data | Compute (cost, I) | Time (I) | Risk |
|---|---|---|---|---|---|
| **(a) Fine-tune Whisper** | Teach an existing Whisper our language and accent | 8 hours gave 63.5 to 32.0 word error on Hindi with Whisper small [F]. About 550 hours took a large-v3 from 61.9 to 32.4 on Common Voice Hindi [F, vendor figure]. About 10,700 hours reached 13.6 average Hindi error [F]. | Whisper small on 8 hours: "5 to 10 hours depending on your GPU" [F]. For 100 to 1,000 hours, tens to a few hundred GPU-hours, roughly USD 100 to 2,000 rented [I, unsourced]. | 2 to 4 weeks | Low to medium |
| **(b) Fine-tune a NeMo Conformer** | Continue training an existing Indian-language Conformer | Same as above | Similar to (a) [I] | 2 to 4 weeks | Medium |
| **(c) Train a streaming transducer from scratch for one language** | A small Zipformer, which allows word-by-word typing | 1,000+ hours of clean transcribed audio | The public English recipe ran 50 epochs on 2 large GPUs (training time not stated) [F]. Plan on about 10 times path (a) [I] | 3 to 6 months | High |
| **(d) A new multilingual model** | Our own foundation model | Tens of thousands of hours | A large cluster | A year or more | **Not realistic for us** |

### 9.2 Step by step (for paths a to c)

1. **Fix the goal.** Languages, accuracy target, speed target, size limit, licences.
2. **Baseline.** Run the open models from section 5 on the test set. If one meets the target, stop here. Do not train.
3. **Collect data.**
   - Public data: IndicVoices (23.7K hours, 11.2K transcribed, 22 languages, CC BY 4.0, gated) [F] https://huggingface.co/datasets/ai4bharat/IndicVoices ; Shrutilipi (6,400+ hours, 12 languages, CC BY 4.0) [F]; Kathbath (1,684 hours, Hindi 150 hours, CC0) [F] https://github.com/AI4Bharat/IndicSUPERB ; FLEURS (102 languages, CC-BY-4.0) [F]; Common Voice Hindi (about 12 hours in v11 [F]; current hours not verified [U]); MUCS 2021 Hindi-English, about 90 hours train [F] with the licence not confirmed [U].
   - Our own data: recordings with clear consent and a clear retention rule. Audio never leaves the user's Mac by default; any collection is opt-in.
4. **Prepare text and audio.** Normalize Unicode (NFC), numerals, punctuation and casing rules; resample to 16 kHz mono; remove bad alignments; split train, dev and test **by speaker** so the test is honest.
5. **Choose the training tool.**
   - Whisper: Hugging Face Transformers fine-tuning, with a published Hindi recipe https://huggingface.co/blog/fine-tune-whisper [F]. LoRA reduces GPU memory [I].
   - NeMo: `speech_to_text_finetune.py`, new tokenizer for a new language, learning rate 1e-4 to 1e-5 [F] https://docs.nvidia.com/nemo/speech/latest/asr/fine_tuning.html
   - Streaming from scratch: `icefall` with `k2` for Zipformer transducers [F] https://github.com/k2-fsa/icefall . ESPnet not checked [U].
6. **Train and watch the dev set.** Save checkpoints. Stop when dev error stops improving.
7. **Evaluate on the untouched test set.** Word and character error, per accent group, noisy audio, Hinglish, names, numbers. Compare with the baseline from step 2.
8. **Export for our app.**
   - Whisper to ggml: `models/convert-h5-to-ggml.py` from whisper.cpp [F].
   - icefall to ONNX: `export-onnx.py` writes encoder, decoder and joiner files for sherpa-onnx [F] https://k2-fsa.github.io/icefall/model-export/export-onnx.html
   - NeMo to sherpa-onnx: export the model, add metadata, quantize to int8, write `tokens.txt` [F].
   - Quantize and check the accuracy loss and the speed on this Mac.
9. **Integrate.** Add it as one more engine behind `SttProvider` or `StreamingStt`, add it to the model manager, add a regression test with a few recordings.
10. **Ship and improve.** Opt-in "report a wrong word" without sending audio by default. Retrain on new data when there is enough.

### 9.3 Team and skills (I)

- 1 machine-learning engineer (training, evaluation).
- 1 data person or linguist per language group (cleaning, transcript checks, text rules).
- Our Rust and app work: the existing team.
- Access to GPUs: rented by the hour is enough for paths (a) and (b).

### 9.4 Honest advice

- **Do (a) before anything bigger,** and only after the baseline shows a gap.
- **Hinglish is the real reason to train:** no open model is proven on it [U], and mixed-language data is scarce.
- **Do not train from scratch** unless streaming word by word in Hindi becomes a hard requirement, because there is no ready streaming Hindi model with a permissive licence [I].

---

## 10. Risks

| Risk | Why it matters | What we do |
|---|---|---|
| Accuracy on accents | Open models are trained on specific audio | Our own test set, per accent, before any claim |
| Speed on this Intel Mac | Bigger models are slower | Measure; quantized models; type per sentence for non-English |
| Licences | Some strong models are non-commercial | Check each licence before adding; keep a licence table in `docs/INSTALLED.md` |
| Model size | 0.5 to 3 GiB downloads | Download on demand, with approval, shown in Settings |
| Typing non-Latin text | Some apps ignore Unicode events; Backspace counts differ | Spike first; grapheme-based counting |
| Hinglish | No proven open model | Plan L3 with data collection |
| Privacy | Voice is personal data | Offline by default; opt-in only for any data collection; never log text or audio |
| Time and cost of own model | Months and real money | Stage gates: only proceed if measurements show a gap |

---

## 11. Decisions needed from you

1. Which languages are first, and which later? (Proposed: English, Hindi, Hinglish first.)
2. What accuracy do you accept for Hindi and for Hinglish? (Proposed starting targets: under 15% and under 20% word error.)
3. May I download one multilingual Whisper for the L1 trial? Proposed: `large-v3-turbo` q5_0 (547 MiB, MIT). Alternative: `small` (466 MiB, Apache-2.0).
4. May I trial IndicWhisper for Hindi in L2? (MIT, medium size, about 1.5 GiB [I].)
5. For Hinglish and your own model: do you want a recording plan with consenting users, and is there a GPU budget? (Fine-tuning is roughly USD 100 to 2,000 of rented compute [I].)
6. Is an optional cloud engine ever acceptable as a plug-in, switched off by default? (Proposed: no, for now.)

---

## 12. Not verified

- Exact current Common Voice Hindi hours.
- The MUCS dataset licence.
- Whether IndicConformer can stream.
- Hindi support in Apple's newer on-device speech API on macOS 26.
- GPU-hour costs (these are my estimates, not sourced).
- Speed of `small`, `medium`, `turbo` and IndicWhisper on this Mac (not measured).

## 13. Sources

- Whisper paper: https://arxiv.org/abs/2212.04356
- Whisper model cards: https://huggingface.co/openai/whisper-large-v3 , https://huggingface.co/openai/whisper-large-v3-turbo
- whisper.cpp models and CLI options: https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md , https://github.com/ggml-org/whisper.cpp/blob/master/examples/cli/README.md
- AI4Bharat IndicConformer: https://huggingface.co/ai4bharat/indic-conformer-600m-multilingual
- Vistaar and IndicWhisper: https://github.com/AI4Bharat/vistaar , https://arxiv.org/abs/2305.15386
- sherpa-onnx models: https://k2-fsa.github.io/sherpa/onnx/pretrained_models/index.html
- Apple feature availability: https://www.apple.com/macos/feature-availability/
- Datasets: https://huggingface.co/datasets/ai4bharat/IndicVoices , https://github.com/AI4Bharat/IndicSUPERB , https://huggingface.co/datasets/google/fleurs , https://openslr.org/104/
- Training: https://huggingface.co/blog/fine-tune-whisper , https://docs.nvidia.com/nemo/speech/latest/asr/fine_tuning.html , https://github.com/k2-fsa/icefall , https://k2-fsa.github.io/icefall/model-export/export-onnx.html
- Language detection: https://huggingface.co/speechbrain/lang-id-voxlingua107-ecapa
- Typing limits: https://github.com/enigo-rs/enigo/issues/68
