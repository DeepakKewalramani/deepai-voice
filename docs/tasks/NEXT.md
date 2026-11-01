# Next tasks (the queue, newest priorities first)

Updated: 1 Nov 2026 by claude-main. Read this with `docs/tasks/BOARD.md` (the slice lock) and `docs/tasks/INBOX.md` (the shared log). Rule: before you start any item, post a `BUILD:` or `RESEARCH:` line in `INBOX.md` (see `AGENTS.md` section 7). If an item says "needs from you", it is blocked until the owner answers.

Status words: NOW (being done), NEXT (ready, nobody started), WAITING (needs a decision or a key), LATER.

## A. Finish what is built (do first)

| ID | Task | Status | Notes |
|---|---|---|---|
| A1 | Review, commit and install the capsule hover controls (2.3c) and the Dictionary (2.15) | NOW | Independent reviews are running. Both edit `lib.rs`, so split that file's changes between the two commits. Then `npm run check`, build, sign with the stable local certificate, install. |
| A2 | **Providers as a main sidebar section (2.17)**: move Providers out of Settings into its own sidebar item (between Dictionary and Styles), no Save and Reset bar there, keep it out of the Settings tab list, update the tests that list the tabs | NEXT (right after A1, same files as A1) | User request, 1 Nov. Reuse the single-tab pattern already used for Styles. Add a small plug icon. Do not edit `ProvidersHub.tsx` or `providers.css`: another agent has uncommitted edits there (see D3). |
| A3 | You test the installed build | WAITING on the owner | Checklist in section E. |

## B. Make connected providers actually work

| ID | Task | Status | Needs from you |
|---|---|---|---|
| B1 | **Cloud adapters (slice 2.7)**: dictation really calls a connected provider. Start with Sarvam (Hindi and Hinglish, `wss://api.sarvam.ai/speech-to-text/ws`, header `Api-Subscription-Key`). Then Soniox, Deepgram, ElevenLabs Scribe, Groq (OpenAI-compatible path), Smallest. Gnani and Reverie stay candidates until verified. | WAITING | A real key for the first provider, and an OK for paid calls (record the price per minute in the slice file). |
| B2 | **Chain editor (rest of slice 2.8)**: order the engines and choose fallbacks (for example Apple streaming, then Sarvam, then Whisper). Connected providers join the chain. | NEXT after B1 starts | None |
| B3 | **Test connection for catalog providers**: add the free metadata call per provider only where the research states one (R.4, R.10). Today only "Make your own" and OpenAI-compatible providers can be tested. | LATER | None |
| B4 | Gnani verification: confirm self-serve keys and the price, then promote it from Candidate | LATER | Someone with access to the Gnani sign-up |

## C. Hindi, other languages, Hinglish (plan: `docs/specs/language-independent-speech.md`)

| ID | Task | Status | Needs from you |
|---|---|---|---|
| C1 | **L1: language setting, model manager, multilingual Whisper** (`large-v3-turbo` q5_0, 547 MiB, or `small`, 466 MiB). Per-language filler and hallucination lists. | WAITING | The six decisions in section 11 of the plan, and an OK to download one model. |
| C2 | **Devanagari typing spike**: type a Hindi sentence into TextEdit, Notes, Mail and a browser, then Backspace part of it. Chunk on grapheme boundaries and count Backspaces in grapheme clusters. | NEXT | None |
| C3 | **Measure before choosing**: speed of Whisper `small`, `medium`, `turbo` on this Mac, and word error on our own test set of Indian-English and Hindi voices (100 sentences per language, 5 or more speakers). | NEXT | Recordings from real people, with consent |
| C4 | **L2: Hindi engine** (IndicWhisper converted to ggml, or IndicConformer through ONNX). Check each licence first. | LATER | An OK to download the model |
| C5 | **L3: Hinglish** fine-tune on code-mixed data (MUCS about 90 hours plus our own recordings). | LATER | A recording plan with consent and a GPU budget |
| C6 | **L4: our own model**, only if C1 to C5 miss the accuracy target. | LATER | A budget and a small team |

## D. Product polish and known gaps

| ID | Task | Status | Notes |
|---|---|---|---|
| D1 | **Info card on double-click (2.16)**: engine in use, app being typed into, live words, permission marks, speed. Display only (no clicks, so no focus theft). | NEXT | Needs the engine name and live words from the dictation code. Not started. |
| D2 | **Reliability gaps from reviews**: fall back to Whisper when a streaming session restart fails mid-hold; extend the Whisper safety net to "segments opened but nothing typed"; a test for restoring the old key after a failed save; a test for the final result kept across a pause restart; capsule one-frame jump after a snap; a press within 0.2 s of a drop. | NEXT | None |
| D3 | **Unclaimed edits to the Providers screen** (brand marks and card styling in `ProvidersHub.tsx` and `providers.css`) | WAITING | Owner must post a `BUILD:` line. Until then nobody edits those two files. |
| D4 | LAN servers: the custom provider form accepts `https://192.168...` but plain `http` only for localhost. Say so in the form and in the code comment. | LATER | None |
| D5 | Keychain status errors: show "Keychain locked" instead of "Key missing"; avoid reading every key on each refresh. | LATER | None |
| D6 | Dark mode screenshots and checks for the Providers, Dictionary and Permissions screens | LATER | None |
| D7 | Toggle dictation mode (slice 2.2 rest): tap to start, tap to stop, separate shortcuts for Dictation and Note taker | LATER | None |

## E. What the owner must test by hand on the installed app

1. Instant typing in your Indian English: how accurate, how fast, and does Esc cancel without deleting text.
2. Capsule: idle is tiny, hover opens Mic and Open buttons, Mic starts and stops dictation with the text cursor staying in the other app, dragging snaps to one of four edges and turns upright on the sides.
3. Dictionary: edit "Deepak", set "Sounds like" to `people`, hold Fn and say "my name is Deepak".
4. Providers: connect a key (macOS may ask for Keychain access), quit and reopen, check it still shows Connected. Try Make your own with Ollama at `http://localhost:11434/v1` and Test connection.
5. Permissions tab: each row jumps to the right System Settings page. Launch at login switch.

## F. Housekeeping

| ID | Task | Status | Notes |
|---|---|---|---|
| F1 | BOARD rows for 2.13 to 2.17 and the planner's roadmap changes | WAITING | The planner session has uncommitted `ROADMAP.md` and `BOARD.md` edits. Update the rows after it commits. |
| F2 | Push `phase-0-foundation` and open the phase PR (a body is drafted in `docs/design/pr-phase1-2-body.md`) | WAITING | Project rule needs no approval phrase, but nothing is pushed until the owner asks. Commits ahead of `origin/phase-0-foundation`: see `git log origin/phase-0-foundation..HEAD`. |
| F3 | Keep `docs/research/INDEX.md` current for every research item | ONGOING | Rule in `AGENTS.md` section 7. |
