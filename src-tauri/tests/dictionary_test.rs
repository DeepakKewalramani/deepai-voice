// Tests for slice 2.15: Dictionary store, validation (shared vectors with the
// UI), Whisper prompt, streaming hints, correction map and the typed result
// through the reconcile path. Words here are test data.

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use deepai_voice_lib::dictation::state::{Effect, Input, Machine};
use deepai_voice_lib::dictation::stream_hold::StreamHold;
use deepai_voice_lib::dictation::stream_text::clusters;
use deepai_voice_lib::dictation::{Outcome, apply_dictionary};
use deepai_voice_lib::dictionary::commands::{
    AddManyReport, ERR_DUPLICATE, ERR_LIMIT, ERR_NOT_FOUND, add_in, add_many_in, delete_in,
    list_in, update_in,
};
use deepai_voice_lib::dictionary::prompt::{
    MAX_HINTS, MAX_PROMPT_CHARS, MAX_PROMPT_WORDS, build_prompt, hint_words,
};
use deepai_voice_lib::dictionary::replace::{Replacer, apply_replacements};
use deepai_voice_lib::dictionary::store::{self, BAD_FILE, FILE, load, save};
use deepai_voice_lib::dictionary::{
    Entry, EntryInput, HoldVocab, MAX_ENTRIES, MAX_HEARD_AS, MAX_HEARD_AS_CHARS, MAX_NOTE_CHARS,
    MAX_WORD_CHARS, applies_to, language_of, validate_heard_as, validate_language, validate_note,
    validate_word,
};
use deepai_voice_lib::hotkey::HotkeyAction;
use deepai_voice_lib::stt::apple_speech::{MAX_CONTEXTUAL_STRINGS, contextual_hints};
use deepai_voice_lib::stt::streaming::FakeStreamingStt;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("deepai-dictionary-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn entry(word: &str, heard: &[&str], language: &str) -> Entry {
    Entry {
        id: format!("id-{word}"),
        word: word.into(),
        heared_as: heard.iter().map(|h| h.to_string()).collect(),
        language: language.into(),
        note: None,
    }
}

fn input(word: &str, heard: &[&str]) -> EntryInput {
    EntryInput {
        word: word.into(),
        heared_as: heard.iter().map(|h| h.to_string()).collect(),
        language: "any".into(),
        note: None,
    }
}

// ---------- store ----------

#[test]
fn missing_file_is_seeded_once_with_two_entries() {
    let dir = temp_dir("seed");
    let path = store::path(&dir);
    let first = load(&path);
    assert!(first.seeded);
    let words: Vec<&str> = first.entries.iter().map(|e| e.word.as_str()).collect();
    assert_eq!(words, ["Deepak", "DeepakKewalramani"]);
    assert!(path.is_file());
    // Second load reads the file; no new seed, same ids.
    let second = load(&path);
    assert!(!second.seeded);
    assert_eq!(second.entries, first.entries);
    // Deleting everything keeps it empty: the seed never comes back.
    for e in &first.entries {
        delete_in(&dir, &e.id).unwrap();
    }
    assert!(list_in(&dir).unwrap().is_empty());
    assert!(!load(&path).seeded);
}

#[test]
fn round_trip_keeps_every_field_and_writes_atomically() {
    let dir = temp_dir("roundtrip");
    let path = store::path(&dir);
    let mut e = entry("दीपक", &["deepak", "dee pack"], "hi");
    e.note = Some("my name".into());
    save(&path, std::slice::from_ref(&e)).unwrap();
    assert!(!dir.join("dictionary.json.tmp").exists());
    let raw: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(raw["version"], 1);
    assert_eq!(raw["entries"][0]["hearedAs"][1], "dee pack");
    let loaded = load(&path);
    assert_eq!(loaded.entries, vec![e]);
    assert!(!loaded.seeded && !loaded.recovered);
}

#[test]
fn invalid_file_falls_back_to_empty_and_keeps_a_bad_copy() {
    for (name, body) in [
        ("garbage", "{not json".to_string()),
        ("version", r#"{"version":2,"entries":[]}"#.to_string()),
    ] {
        let dir = temp_dir(name);
        let path = dir.join(FILE);
        fs::write(&path, &body).unwrap();
        let loaded = load(&path);
        assert!(loaded.recovered, "{name}");
        assert!(loaded.entries.is_empty());
        assert_eq!(fs::read_to_string(dir.join(BAD_FILE)).unwrap(), body);
        // The file now holds an empty dictionary, not the seed.
        let again = load(&path);
        assert!(!again.seeded && !again.recovered && again.entries.is_empty());
    }
}

#[test]
fn invalid_entries_are_dropped_and_the_original_kept() {
    let dir = temp_dir("partial");
    let path = dir.join(FILE);
    let long = "a".repeat(61);
    let body = format!(
        r#"{{"version":1,"entries":[
        {{"id":"1","word":"Deepak","hearedAs":["people"],"language":"any"}},
        {{"id":"2","word":"deepak","language":"any"}},
        {{"id":"3","word":"{long}","language":"any"}}]}}"#
    );
    fs::write(&path, &body).unwrap();
    let loaded = load(&path);
    assert!(loaded.recovered);
    assert_eq!(loaded.entries.len(), 1);
    assert_eq!(loaded.entries[0].heared_as, vec!["people".to_string()]);
    assert_eq!(fs::read_to_string(dir.join(BAD_FILE)).unwrap(), body);
}

// ---------- validation (shared vectors with the UI) ----------

const VECTORS: &str = include_str!("../../src/dictionary/validation-vectors.json");

fn strings(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn shared_vectors_match_rust_validation() {
    let v: serde_json::Value = serde_json::from_str(VECTORS).unwrap();
    let limits = &v["limits"];
    assert_eq!(limits["maxEntries"], MAX_ENTRIES);
    assert_eq!(limits["maxWordChars"], MAX_WORD_CHARS);
    assert_eq!(limits["maxHeardAs"], MAX_HEARD_AS);
    assert_eq!(limits["maxHeardAsChars"], MAX_HEARD_AS_CHARS);
    assert_eq!(limits["maxNoteChars"], MAX_NOTE_CHARS);

    for pair in v["word"]["valid"].as_array().unwrap() {
        let (raw, want) = (pair[0].as_str().unwrap(), pair[1].as_str().unwrap());
        assert_eq!(validate_word(raw).as_deref(), Ok(want), "{raw:?}");
    }
    for raw in strings(&v["word"]["invalid"]) {
        assert!(validate_word(&raw).is_err(), "{raw:?}");
    }
    for pair in v["heardAs"]["valid"].as_array().unwrap() {
        let (raw, want) = (strings(&pair[0]), strings(&pair[1]));
        assert_eq!(validate_heard_as(&raw), Ok(want), "{raw:?}");
    }
    for raw in v["heardAs"]["invalid"].as_array().unwrap() {
        assert!(validate_heard_as(&strings(raw)).is_err(), "{raw:?}");
    }
    for raw in strings(&v["language"]["valid"]) {
        assert_eq!(validate_language(&raw).as_deref(), Ok(raw.as_str()));
    }
    for raw in strings(&v["language"]["invalid"]) {
        assert!(validate_language(&raw).is_err(), "{raw:?}");
    }
    for pair in v["note"]["valid"].as_array().unwrap() {
        let raw = pair[0].as_str().unwrap();
        assert_eq!(
            validate_note(Some(raw)),
            Ok(pair[1].as_str().map(String::from))
        );
    }
    for raw in strings(&v["note"]["invalid"]) {
        assert!(validate_note(Some(&raw)).is_err(), "{raw:?}");
    }
}

// ---------- commands ----------

#[test]
fn add_rejects_case_insensitive_duplicates_without_echoing_the_word() {
    let dir = temp_dir("dup");
    let added = add_in(&dir, &input("Secretword", &["secret ward"])).unwrap();
    assert_eq!(added.word, "Secretword");
    let err = add_in(&dir, &input("  SECRETWORD ", &[])).unwrap_err();
    assert_eq!(err, ERR_DUPLICATE);
    assert!(!err.to_lowercase().contains("secret"));
    let bad = add_in(&dir, &input(&"x".repeat(61), &[])).unwrap_err();
    assert!(!bad.contains("xxx"));
}

#[test]
fn update_and_delete_by_id() {
    let dir = temp_dir("update");
    let a = add_in(&dir, &input("Alpha", &[])).unwrap();
    let b = add_in(&dir, &input("Beta", &[])).unwrap();
    // Renaming to another entry's word is a duplicate; to its own case is fine.
    assert_eq!(
        update_in(&dir, &b.id, &input("alpha", &[])).unwrap_err(),
        ERR_DUPLICATE
    );
    let b2 = update_in(&dir, &b.id, &input("BETA", &["bay ta"])).unwrap();
    assert_eq!(b2.id, b.id);
    assert_eq!(b2.heared_as, vec!["bay ta".to_string()]);
    assert_eq!(
        update_in(&dir, "nope", &input("Gamma", &[])).unwrap_err(),
        ERR_NOT_FOUND
    );
    delete_in(&dir, &a.id).unwrap();
    assert_eq!(delete_in(&dir, &a.id).unwrap_err(), ERR_NOT_FOUND);
    let left: Vec<String> = list_in(&dir).unwrap().into_iter().map(|e| e.word).collect();
    assert!(left.contains(&"BETA".to_string()) && !left.contains(&"Alpha".to_string()));
}

#[test]
fn limit_is_500_entries() {
    let dir = temp_dir("limit");
    let text: String = (0..MAX_ENTRIES + 3).map(|i| format!("word{i}\n")).collect();
    // The two seed entries are there first.
    let report = add_many_in(&dir, &text, "en").unwrap();
    assert_eq!(report.added, MAX_ENTRIES - 2);
    assert_eq!(report.over_limit, 5);
    assert_eq!(list_in(&dir).unwrap().len(), MAX_ENTRIES);
    assert_eq!(
        add_in(&dir, &input("one more", &[])).unwrap_err(),
        ERR_LIMIT
    );
}

#[test]
fn add_many_ignores_blank_lines_and_counts_duplicates_and_invalid() {
    let dir = temp_dir("many");
    let long = "z".repeat(61);
    let text = format!("Kotlin\n\n   \nkotlin\nDeepak\n  Rust  \n{long}\r\nTauri\r\n");
    let report = add_many_in(&dir, &text, "any").unwrap();
    assert_eq!(
        report,
        AddManyReport {
            added: 3,
            duplicates: 2,
            invalid: 1,
            over_limit: 0
        }
    );
    let words: Vec<String> = list_in(&dir).unwrap().into_iter().map(|e| e.word).collect();
    assert_eq!(words, ["Deepak", "DeepakKewalramani", "Kotlin", "Rust", "Tauri"]);
    assert!(add_many_in(&dir, "x", "English").is_err());
}

// ---------- prompt and hints ----------

#[test]
fn prompt_lists_words_for_the_language_without_heard_as() {
    let entries = vec![
        entry("Deepak", &["people"], "any"),
        entry("DeepakKewalramani", &["tech city"], "en"),
        entry("नमस्ते", &[], "hi"),
        entry("deepak", &[], "any"),
    ];
    let p = build_prompt(&entries, "en");
    assert_eq!(p, "Vocabulary: Deepak, DeepakKewalramani");
    assert!(!p.contains("people") && !p.contains("tech city"));
    assert_eq!(build_prompt(&entries, "hi"), "Vocabulary: Deepak, नमस्ते");
    assert_eq!(build_prompt(&[], "en"), "");
    assert_eq!(build_prompt(&[entry("Hindi", &[], "hi")], "en"), "");
    assert_eq!(hint_words(&entries, "en-IN"), ["Deepak", "DeepakKewalramani"]);
}

#[test]
fn prompt_is_capped_in_words_and_characters() {
    let many: Vec<Entry> = (0..200)
        .map(|i| entry(&format!("Term{i}"), &[], "any"))
        .collect();
    let p = build_prompt(&many, "en");
    let terms = p.trim_start_matches("Vocabulary: ").split(", ").count();
    assert_eq!(terms, MAX_PROMPT_WORDS);
    let long: Vec<Entry> = (0..40)
        .map(|i| entry(&format!("{}{i}", "a".repeat(50)), &[], "any"))
        .collect();
    let p = build_prompt(&long, "en");
    assert!(p.chars().count() <= MAX_PROMPT_CHARS, "{}", p.len());
    assert!(p.split(", ").count() >= 6);
    assert_eq!(hint_words(&many, "en").len(), MAX_HINTS);
}

#[test]
fn language_matching() {
    assert_eq!(language_of("en-IN"), "en");
    assert_eq!(language_of("hi_IN"), "hi");
    assert!(applies_to("any", "hi"));
    assert!(applies_to("en", "en-US"));
    assert!(!applies_to("hi", "en"));
}

#[test]
fn contextual_hints_are_deduplicated_and_capped() {
    let mut hints: Vec<String> = (0..150).map(|i| format!("w{i}")).collect();
    hints.insert(0, "W1".into());
    hints.insert(0, "  ".into());
    let out = contextual_hints(&hints);
    assert_eq!(out.len(), MAX_CONTEXTUAL_STRINGS);
    assert_eq!(out[0], "W1");
    assert!(!out.contains(&"w1".to_string()));
}

// ---------- correction map ----------

fn fix(text: &str, entries: &[Entry]) -> String {
    apply_replacements(text, entries)
}

#[test]
fn replaces_whole_words_case_insensitively() {
    let e = [entry("Deepak", &["people"], "any")];
    assert_eq!(fix("my name is people", &e), "my name is Deepak");
    assert_eq!(fix("my name is PEOPLE.", &e), "my name is Deepak.");
    assert_eq!(fix("people, people!", &e), "Deepak, Deepak!");
    // Inside a longer word: untouched.
    assert_eq!(
        fix("peoples peoplehood unpeople", &e),
        "peoples peoplehood unpeople"
    );
    assert_eq!(fix("people's", &e), "people's");
    assert_eq!(fix("", &e), "");
    assert_eq!(fix("my name is people", &[]), "my name is people");
}

#[test]
fn keeps_a_capital_at_sentence_start_only() {
    let e = [entry("iPhone", &["eye phone"], "any")];
    // A spelling chosen with capitals is kept as typed, even at a sentence start.
    assert_eq!(fix("Eye phone is here", &e), "iPhone is here");
    assert_eq!(fix("ok. Eye phone", &e), "ok. iPhone");
    assert_eq!(fix("my Eye phone", &e), "my iPhone");
    assert_eq!(fix("eye phone", &e), "iPhone");
}

#[test]
fn multi_word_spellings_need_plain_spaces_and_longest_wins() {
    let e = [
        entry("DeepakKewalramani", &["tech city", "tech"], "any"),
        entry("Deepak", &["dee pack"], "any"),
    ];
    assert_eq!(
        fix("we work at tech city now", &e),
        "we work at DeepakKewalramani now"
    );
    assert_eq!(fix("tech  city", &e), "DeepakKewalramani");
    assert_eq!(fix("tech, city", &e), "DeepakKewalramani, city");
    assert_eq!(fix("I am Dee Pack", &e), "I am Deepak");
}

#[test]
fn replacements_never_chain() {
    let e = [
        entry("beta", &["alpha"], "any"),
        entry("gamma", &["beta"], "any"),
    ];
    assert_eq!(fix("alpha beta", &e), "beta gamma");
    let r = Replacer::new(&e);
    assert_eq!(r.apply("alpha"), "beta");
}

#[test]
fn unicode_words_and_devanagari() {
    let e = [
        entry("दीपक", &["दिपक"], "hi"),
        entry("Zoë", &["zoe"], "any"),
        entry("Deepak", &["café"], "any"),
    ];
    assert_eq!(fix("मेरा नाम दिपक है।", &e), "मेरा नाम दीपक है।");
    // A vowel sign continues the word: "दिपकों" is a longer word.
    assert_eq!(fix("दिपकों", &e), "दिपकों");
    assert_eq!(fix("hi zoe!", &e), "hi Zoë!");
    assert_eq!(fix("zoey", &e), "zoey");
    assert_eq!(fix("a café there", &e), "a Deepak there");
    assert_eq!(fix("cafés", &e), "cafés");
}

// ---------- dictation wiring ----------

fn vocab(entries: &[Entry]) -> Arc<HoldVocab> {
    Arc::new(HoldVocab::from_entries(entries, "en"))
}

#[test]
fn hold_vocab_filters_language_and_reads_the_store() {
    let v = HoldVocab::from_entries(
        &[
            entry("Deepak", &["people"], "any"),
            entry("दीपक", &["दिपक"], "hi"),
        ],
        "en",
    );
    assert_eq!(v.prompt, "Vocabulary: Deepak");
    assert_eq!(v.whisper_hint(), Some("Vocabulary: Deepak"));
    assert_eq!(v.hints, ["Deepak"]);
    assert_eq!(v.correct("दिपक people"), "दिपक Deepak");
    assert_eq!(HoldVocab::default().whisper_hint(), None);
    assert_eq!(HoldVocab::load(None, "en").whisper_hint(), None);

    let dir = temp_dir("holdvocab");
    add_in(&dir, &input("Kotlin", &["cot lin"])).unwrap();
    let loaded = HoldVocab::load(Some(&dir), "en");
    assert_eq!(loaded.prompt, "Vocabulary: Deepak, DeepakKewalramani, Kotlin");
    assert_eq!(loaded.correct("cot lin"), "Kotlin");
}

#[test]
fn whisper_partials_and_finals_are_corrected() {
    let v = vocab(&[entry("Deepak", &["people"], "any")]);
    let p = apply_dictionary(
        Input::Partial {
            seq: 2,
            text: "hi people".into(),
        },
        &v,
    );
    assert_eq!(
        p,
        Input::Partial {
            seq: 2,
            text: "hi Deepak".into()
        }
    );
    let f = apply_dictionary(Input::Transcribed(Outcome::Text("People here".into())), &v);
    assert_eq!(f, Input::Transcribed(Outcome::Text("Deepak here".into())));
    let s = apply_dictionary(
        Input::Segment {
            seq: 1,
            outcome: Outcome::Nothing,
        },
        &v,
    );
    assert_eq!(
        s,
        Input::Segment {
            seq: 1,
            outcome: Outcome::Nothing
        }
    );
    assert_eq!(apply_dictionary(Input::Cancel, &v), Input::Cancel);
}

#[test]
fn fake_streaming_engine_receives_the_hints() {
    let engine = Arc::new(FakeStreamingStt::new(&["hello"]));
    let v = vocab(&[
        entry("Deepak", &["people"], "any"),
        entry("DeepakKewalramani", &[], "en"),
        entry("नमस्ते", &[], "hi"),
    ]);
    let _h = StreamHold::start_with_vocab(engine.clone(), "en-IN", v).unwrap();
    assert_eq!(engine.hints_seen(), vec![vec!["Deepak", "DeepakKewalramani"]]);
    // Without a dictionary, the default start passes no hints.
    let plain = Arc::new(FakeStreamingStt::new(&["hello"]));
    let _p = StreamHold::start(plain.clone(), "en-US").unwrap();
    assert_eq!(plain.hints_seen(), vec![Vec::<String>::new()]);
}

const VOICE: f32 = 0.1;

fn voice(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| if i % 2 == 0 { VOICE } else { -VOICE })
        .collect()
}

/// Types effects into `screen` like the typing thread, answering each one.
/// Returns every effect, in order.
fn type_into(m: &mut Machine, inputs: Vec<Input>, screen: &mut String) -> Vec<Effect> {
    let mut seen = Vec::new();
    for input in inputs {
        let mut pending: VecDeque<Effect> = m.handle(input).into_iter().collect();
        while let Some(e) = m.take_followup() {
            pending.push_back(e);
        }
        while let Some(effect) = pending.pop_front() {
            match &effect {
                Effect::Paste(t) => screen.push_str(t),
                Effect::Correct { backspaces, text } => {
                    let keep = clusters(screen).len() - backspaces;
                    *screen = clusters(screen)[..keep].concat();
                    screen.push_str(text);
                }
                _ => {
                    seen.push(effect);
                    continue;
                }
            }
            seen.push(effect);
            pending.extend(m.handle(Input::Pasted(Ok(()))));
            while let Some(e) = m.take_followup() {
                pending.push_back(e);
            }
        }
    }
    seen
}

fn dictate(words: &[&str], final_text: &str, entries: &[Entry]) -> (String, Vec<Effect>) {
    let engine = Arc::new(FakeStreamingStt::new(words).with_final(final_text));
    let mut hold = StreamHold::start_with_vocab(engine, "en-IN", vocab(entries)).unwrap();
    let mut m = Machine::new();
    m.handle(Input::Hotkey(HotkeyAction::StartRecording));
    let mut screen = String::new();
    let mut effects = Vec::new();
    for _ in words {
        let inputs = hold.feed(&voice(1600));
        effects.extend(type_into(&mut m, inputs, &mut screen));
    }
    m.handle(Input::Hotkey(HotkeyAction::StopRecording));
    let rel = hold.release(&[]);
    effects.extend(type_into(&mut m, rel, &mut screen));
    (screen, effects)
}

#[test]
fn my_name_is_people_is_typed_as_deepak() {
    let e = [entry("Deepak", &["people"], "any")];
    let (screen, effects) = dictate(
        &["my", "name", "is", "people", "and", "so"],
        "my name is people",
        &e,
    );
    assert_eq!(screen, "my name is Deepak");
    // No raw spelling was ever typed.
    for eff in &effects {
        if let Effect::Paste(t) | Effect::Correct { text: t, .. } = eff {
            assert!(!t.contains("people"), "{eff:?}");
        }
    }
    // The revision removed only " and" (typed after "Deepak").
    assert_eq!(
        effects.last(),
        Some(&Effect::Correct {
            backspaces: 4,
            text: String::new()
        })
    );
}

#[test]
fn backspace_counts_use_the_corrected_text() {
    // Typed "we work at DeepakKewalramani now" (corrected); the revision drops "city",
    // so the corrected word itself must be erased: 11 Backspaces on the
    // corrected text ("DeepakKewalramani now"), not 13 on the raw "tech city now".
    let e = [entry("DeepakKewalramani", &["tech city"], "any")];
    let (screen, effects) = dictate(
        &["we", "work", "at", "tech", "city", "now", "ok"],
        "we work at tech",
        &e,
    );
    assert_eq!(screen, "we work at tech");
    assert_eq!(
        effects.last(),
        Some(&Effect::Correct {
            backspaces: 11,
            text: "tech".into()
        })
    );
}

#[test]
fn plain_lowercase_words_are_capitalised_at_a_sentence_start() {
    let e = [entry("deepak", &["people"], "any")];
    assert_eq!(fix("People are here", &e), "Deepak are here");
    assert_eq!(fix("see people", &e), "see deepak");
}
