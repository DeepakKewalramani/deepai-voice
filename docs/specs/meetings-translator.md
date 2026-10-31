# DeepAI Voice: Meeting notes and Live translator modules

Status: Gate 2 draft, 31 Oct 2026. Gates 1 to 3 approved by user (31 Oct 2026). Design: `docs/adr/0003-meetings-translator.md`. Facts: `docs/research/meetings-translator-findings.md`.
Build target: the user's Intel Mac only (x86_64, macOS 15.7.4) until other targets are approved.

## Context
Right now, a person on a call in any app has to take notes by hand and, when the other side speaks another language, has to guess or use a separate tool. They lose details and cannot reply well in that language.

## What it does
Meeting notes:
1. One toggle "Record this call" (floating bar right button, tray or shortcut) captures mic ("You") and system audio ("Others") from any app.
2. Live transcript per channel in any language (auto-detected); speaker split by channel, further by diarization where the provider supports it.
3. On stop: summary, decisions, action items; session saved locally; full-text search; export Markdown.

Live translator:
4. Live subtitles of the other side in the user's chosen languages, with the original kept. An optional "simple wording" mode.
5. Suggested replies based on the whole conversation so far, shown in the user's language plus the target language. The user picks or edits one.
6. The user writes or speaks their own reply in any language, and the app translates it.
7. Send: paste to chat, or speak it into the call in the target language (TTS to the "DeepAI Voice Mic" virtual microphone). Speaking out loud locally is also an option.
8. The full bilingual transcript is saved and nothing is lost. A glossary keeps terms consistent.

## Business rules
1. Languages are settings: input language auto or chosen; one or more output languages. None hard-coded.
2. Capture starts only after the user turns it on and confirms consent (first use). A banner is shown the whole time capture is on.
3. Audio is not written to disk unless "Keep audio" is on (default off). Text segments are stored in local SQLite.
4. Every segment stores the original text, detected language, translations, channel, timestamps and provider.
5. Glossary terms override MT output; the same source term always gives the same target term inside a session.
6. Translation and suggestion failures fall back down the provider chain (ADR 0002 rule 14). On full failure the original is shown with an "untranslated" label; nothing is dropped.
7. Suggested replies never auto-send. Every send (paste or speak) is an explicit user action.
8. Virtual mic output plays only while the user triggers "Speak"; the user's real mic is not muted automatically (DEFAULT, Q21).
9. Logs hold no transcript, translation or audio. Lengths and timings only (dictation rule 12).
10. Privacy notice before the first cloud use of system audio, translation and TTS (dictation rule 15).

## What NOT to build
- Bots that join meetings, per-app integrations or SDKs (Zoom, Meet, Teams, WeChat).
- Bundling BlackHole or any GPL driver in the app.
- Cloud sync, accounts, sharing links, team spaces.
- Video or screen recording.
- Automatic sending of replies; auto-muting the user's mic.
- Voice cloning of the user.
- Google Docs export (later).
- Any target other than the Intel Mac before user approval.
- Legal advice in the app; the consent text is a reminder only.

## Done looks like (Intel Mac)
- [ ] With Zoom, Google Meet in Chrome and a YouTube video as sources, "Record this call" captures system audio and mic as separate channels (level meters move independently).
- [ ] Live text appears for both channels; the median delay from speech end to text is measured and recorded (target under 2 s is INFERENCE, set after spike S9).
- [ ] A Hindi or Chinese test clip on the system channel shows subtitles in English (or the chosen language) with the original kept.
- [ ] Changing the output language in Settings takes effect for the next segment without a restart.
- [ ] Stop gives a summary, decisions and action items; the session reopens after an app restart; search finds a term in both the original and the translation.
- [ ] Markdown export contains all segments with speaker side, time, original and translation.
- [ ] Suggested replies: 2 to 3 options appear in both languages within the measured time; picking one fills the editor and does not send it.
- [ ] A typed reply in Hindi is translated to English; "Paste" puts it into the chat box of the call app.
- [ ] "Speak": the other side of a test call hears the reply in the target language through "DeepAI Voice Mic" (or BlackHole during the prototype).
- [ ] Glossary: 5 terms are translated identically across 3 sessions.
- [ ] The consent screen appears on first use and the banner is visible while capturing.
- [ ] Network off: capture keeps going; segments are stored as "untranslated"/"pending" and nothing is lost.
- [ ] No audio files on disk with "Keep audio" off (same find check as dictation); no transcript text in logs.

## Open questions
Q20 to Q26 in HUMAN_INPUT.md.
