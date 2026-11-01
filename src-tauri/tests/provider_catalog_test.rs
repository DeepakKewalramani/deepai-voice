//! Provider hub (slices 2.13 and 2.14): catalog invariants, custom provider
//! validation, and the connect / disconnect / custom flows against a fake key store.
//! The secret used here must never appear in settings.json, results or errors.

use deepai_voice_lib::providers::catalog::{CATALOG, find};
use deepai_voice_lib::providers::custom::{
    CustomProviderForm, validate, validate_base_url, validate_header_name, validate_model,
    validate_name,
};
use deepai_voice_lib::providers::http::UreqProbe;
use deepai_voice_lib::providers::hub::{Hub, KeyState, key_ref};
use deepai_voice_lib::providers::probe::{
    HttpProbe, NoHttpClient, ProbeTarget, REASON_NO_FREE_CHECK, REASON_NOT_HTTPS, TestResult,
};
use deepai_voice_lib::providers::{AuthStyle, Capability, EntryStatus, Setup, Tri};
use deepai_voice_lib::settings::keys::{KeyError, KeyStore, MemoryKeyStore};
use deepai_voice_lib::settings::{self, Settings, load, save, settings_path};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SECRET: &str = "sk-test-SECRET-9f8e7d6c5b4a1234";

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "deepai-providers-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Fake credential store that records every write. Never touches the Keychain.
#[derive(Default)]
struct RecordingStore {
    inner: MemoryKeyStore,
    writes: Mutex<Vec<(String, String)>>,
    deletes: Mutex<Vec<String>>,
    fail_writes: bool,
}

impl KeyStore for RecordingStore {
    fn get_key(&self, key_ref: &str) -> Result<Option<String>, KeyError> {
        self.inner.get_key(key_ref)
    }
    fn set_key(&self, key_ref: &str, secret: &str) -> Result<(), KeyError> {
        if self.fail_writes {
            // A backend error that (badly) echoes the secret: the hub must not pass it on.
            return Err(KeyError(format!("backend refused {secret}")));
        }
        self.writes
            .lock()
            .unwrap()
            .push((key_ref.into(), secret.into()));
        self.inner.set_key(key_ref, secret)
    }
    fn delete_key(&self, key_ref: &str) -> Result<(), KeyError> {
        self.deletes.lock().unwrap().push(key_ref.into());
        self.inner.delete_key(key_ref)
    }
}

fn vectors() -> Value {
    serde_json::from_str(include_str!(
        "../../src/settings/providers/validation-vectors.json"
    ))
    .unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

fn form(name: &str, url: &str, model: &str, auth: &str) -> CustomProviderForm {
    CustomProviderForm {
        id: None,
        capability: "llm".into(),
        name: name.into(),
        base_url: url.into(),
        model: model.into(),
        auth_style: auth.into(),
        header_name: String::new(),
        protocol: String::new(),
    }
}

// ---------- catalog ----------

#[test]
fn catalog_ids_are_unique_and_simple() {
    let mut seen = HashSet::new();
    for e in CATALOG {
        assert!(seen.insert(e.id), "duplicate id {}", e.id);
        assert!(
            e.id.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{}",
            e.id
        );
        assert!(
            !e.id.starts_with("custom-"),
            "{} uses the custom prefix",
            e.id
        );
        assert_eq!(find(e.id).map(|f| f.id), Some(e.id));
    }
    assert!(find("nope").is_none());
}

#[test]
fn catalog_docs_urls_are_https_without_spaces() {
    for e in CATALOG {
        assert!(e.docs_url.starts_with("https://"), "{}", e.id);
        assert!(!e.docs_url.chars().any(char::is_whitespace), "{}", e.id);
        if let Some(base) = e.base_url_default {
            assert!(validate_base_url(base).is_ok(), "{} base {base}", e.id);
        }
    }
}

#[test]
fn catalog_enums_are_consistent() {
    for e in CATALOG {
        assert!(!e.name.is_empty() && !e.vendor.is_empty() && !e.note.is_empty());
        assert!(!e.price.is_empty() && !e.languages.is_empty(), "{}", e.id);
        match e.status {
            EntryStatus::Builtin => assert_eq!(e.setup, Setup::None, "{}", e.id),
            EntryStatus::Candidate => assert_eq!(e.setup, Setup::None, "{}", e.id),
            EntryStatus::Verified => assert_ne!(e.setup, Setup::None, "{}", e.id),
        }
        if e.setup == Setup::Key {
            assert!(
                !matches!(e.auth, AuthStyle::None),
                "{} needs a key but has no auth",
                e.id
            );
        }
        // Every enum serialises to a known lowercase string.
        let v = serde_json::to_value(e).unwrap();
        for (field, allowed) in [
            ("capability", &["stt", "llm", "tts"][..]),
            ("hindi", &["yes", "no", "unknown"][..]),
            ("streaming", &["yes", "no", "unknown"][..]),
            ("status", &["builtin", "verified", "candidate"][..]),
            ("setup", &["none", "key", "custom"][..]),
        ] {
            let s = v[field].as_str().unwrap();
            assert!(allowed.contains(&s), "{} {field}={s}", e.id);
        }
        let auth = v["authStyle"].as_str().unwrap();
        assert!(
            ["bearer", "token", "x-api-key", "none", "unknown"].contains(&auth)
                || auth.starts_with("header:"),
            "{} auth {auth}",
            e.id
        );
    }
}

#[test]
fn catalog_has_each_capability_and_the_requested_vendors() {
    for cap in [Capability::Stt, Capability::Llm, Capability::Tts] {
        assert!(CATALOG.iter().any(|e| e.capability == cap));
    }
    for id in [
        "whisper-local",
        "apple-speech",
        "sarvam-saaras",
        "deepgram-nova-3",
        "elevenlabs-scribe",
        "assemblyai",
        "groq-whisper",
        "openai-compatible-stt",
        "gnani",
        "soniox",
        "smallest-pulse",
        "reverie",
        "anthropic",
        "openai-compatible-llm",
        "ollama",
        "elevenlabs-tts",
        "sarvam-bulbul",
        "smallest-lightning",
        "macos-voice",
    ] {
        assert!(find(id).is_some(), "missing {id}");
    }
    assert_eq!(find("sarvam-saaras").unwrap().hindi, Tri::Yes);
    assert_eq!(find("soniox").unwrap().status, EntryStatus::Verified);
    for id in ["smallest-pulse", "smallest-lightning"] {
        assert!(find(id).unwrap().price.contains("conflict"), "{id}");
    }
    // R.10 does not recommend these; they must not be listed.
    for e in CATALOG {
        let v = e.vendor.to_lowercase();
        for banned in ["bhashini", "google", "azure", "speechmatics"] {
            assert!(!v.contains(banned), "{} is {banned}", e.id);
        }
    }
}

#[test]
fn catalog_has_no_openai_tts_or_shut_down_speech_models() {
    for e in CATALOG {
        let text = format!("{} {} {} {}", e.id, e.name, e.default_model, e.note).to_lowercase();
        for banned in [
            "tts-1",
            "gpt-4o-mini-tts",
            "whisper-1",
            "gpt-4o-transcribe",
            "/audio/speech",
        ] {
            assert!(!text.contains(banned), "{} mentions {banned}", e.id);
        }
        if e.capability == Capability::Tts {
            assert!(!e.vendor.to_lowercase().contains("openai"), "{}", e.id);
        }
    }
}

#[test]
fn candidates_are_flagged_and_cannot_connect() {
    let gnani = find("gnani").unwrap();
    assert_eq!(gnani.status, EntryStatus::Candidate);
    assert_eq!(gnani.auth, AuthStyle::Header("X-API-Key-ID".into()));
    let note = gnani.note.to_lowercase();
    assert!(note.contains("not verified") && note.contains("contradict"));
    assert!(note.contains("not openai-compatible"));
    let reverie = find("reverie").unwrap();
    assert_eq!(reverie.status, EntryStatus::Candidate);
    assert_eq!(reverie.setup, Setup::None);
    assert!(reverie.note.contains("query string"));
    let dir = temp_dir("candidate");
    let store = RecordingStore::default();
    let hub = Hub::new(settings_path(&dir), &store);
    let err = hub.connect("gnani", SECRET).unwrap_err();
    assert!(!err.contains(SECRET));
    assert!(store.writes.lock().unwrap().is_empty());
    assert!(hub.connect("reverie", SECRET).is_err());
    assert!(hub.connect("whisper-local", SECRET).is_err());
    assert!(hub.connect("openai-compatible-llm", SECRET).is_err());
    assert!(hub.connect("unknown-id", SECRET).is_err());
}

// ---------- custom validation ----------

#[test]
fn base_url_vectors_shared_with_the_ui() {
    let v = vectors();
    for pair in v["baseUrl"]["valid"].as_array().unwrap() {
        let (input, want) = (pair[0].as_str().unwrap(), pair[1].as_str().unwrap());
        assert_eq!(validate_base_url(input).as_deref(), Ok(want), "{input}");
    }
    for bad in strings(&v["baseUrl"]["invalid"]) {
        let err = validate_base_url(&bad).expect_err(&bad);
        assert!(!err.is_empty());
        if bad.len() > 12 {
            assert!(!err.contains(&bad), "error echoes input");
        }
    }
}

#[test]
fn name_model_header_vectors_shared_with_the_ui() {
    let v = vectors();
    for s in strings(&v["name"]["valid"]) {
        assert!(validate_name(&s).is_ok(), "{s}");
    }
    for s in strings(&v["name"]["invalid"]) {
        assert!(validate_name(&s).is_err(), "{s:?}");
    }
    for s in strings(&v["model"]["valid"]) {
        assert!(validate_model(&s).is_ok(), "{s}");
    }
    for s in strings(&v["model"]["invalid"]) {
        assert!(validate_model(&s).is_err(), "{s:?}");
    }
    for s in strings(&v["headerName"]["valid"]) {
        assert!(validate_header_name(&s).is_ok(), "{s}");
    }
    for s in strings(&v["headerName"]["invalid"]) {
        assert!(validate_header_name(&s).is_err(), "{s:?}");
    }
}

#[test]
fn form_validation_defaults_and_rejections() {
    let ok = validate(&form(
        "Ollama",
        "http://localhost:11434/v1/",
        "llama3.2",
        "",
    ))
    .unwrap();
    assert_eq!(ok.auth, AuthStyle::Bearer, "bearer is the default");
    assert_eq!(ok.base_url, "http://localhost:11434/v1");
    assert_eq!(ok.capability, Capability::Llm);
    assert!(ok.is_local());

    let mut f = form("Mine", "https://api.example.com/v1", "m", "custom");
    f.header_name = "api-subscription-key".into();
    let v = validate(&f).unwrap();
    assert_eq!(v.auth, AuthStyle::Header("api-subscription-key".into()));
    assert!(!v.is_local());

    f.header_name = "Bad Header".into();
    assert_eq!(validate(&f).unwrap_err().field, "headerName");

    let mut f = form("Mine", "https://api.example.com/v1", "m", "bearer");
    f.capability = "video".into();
    assert_eq!(validate(&f).unwrap_err().field, "capability");
    f.capability = "tts".into();
    f.protocol = "grpc".into();
    assert_eq!(validate(&f).unwrap_err().field, "protocol");
    f.protocol = "openai-compatible".into();
    assert!(validate(&f).is_ok());
    f.auth_style = "basic".into();
    assert_eq!(validate(&f).unwrap_err().field, "authStyle");
    assert_eq!(
        validate(&form("", "https://a.b", "m", ""))
            .unwrap_err()
            .field,
        "name"
    );
    assert_eq!(
        validate(&form("n", "javascript:alert(1)", "m", ""))
            .unwrap_err()
            .field,
        "baseUrl"
    );
    assert_eq!(
        validate(&form("n", "https://a.b", "a b", ""))
            .unwrap_err()
            .field,
        "model"
    );
}

// ---------- key storage and flows ----------

fn assert_no_secret(text: &str) {
    assert!(!text.contains(SECRET), "secret leaked: {text}");
    assert!(!text.contains("SECRET-9f8e"), "secret fragment leaked");
}

#[test]
fn connect_stores_key_only_in_the_credential_store() {
    let dir = temp_dir("connect");
    let path = settings_path(&dir);
    let store = RecordingStore::default();
    let hub = Hub::new(path.clone(), &store);

    let status = hub.connect("sarvam-saaras", SECRET).unwrap();
    assert!(status.connected);
    assert_eq!(status.key, KeyState::Set);
    assert_eq!(status.key_tail.as_deref(), Some("1234"));
    assert_no_secret(&serde_json::to_string(&status).unwrap());

    assert_eq!(
        *store.writes.lock().unwrap(),
        vec![(key_ref("sarvam-saaras"), SECRET.to_owned())]
    );
    assert_eq!(key_ref("sarvam-saaras"), "provider:sarvam-saaras");

    let text = fs::read_to_string(&path).unwrap();
    assert_no_secret(&text);
    let saved = load(&path).settings;
    let p = saved
        .providers
        .iter()
        .find(|p| p.id == "sarvam-saaras")
        .unwrap();
    assert_eq!(p.kind, "stt");
    assert_eq!(p.adapter, "sarvam");
    assert_eq!(p.key_ref.as_deref(), Some("provider:sarvam-saaras"));
    // The built-in Whisper entry is kept.
    assert!(saved.providers.iter().any(|p| p.id == "whisper-local"));

    let all = hub.status().unwrap();
    assert_no_secret(&serde_json::to_string(&all).unwrap());
    let s = all.iter().find(|s| s.id == "sarvam-saaras").unwrap();
    assert!(s.connected);
    assert_eq!(s.key_tail.as_deref(), Some("1234"));
}

#[test]
fn reconnect_replaces_the_key_without_duplicating_the_entry() {
    let dir = temp_dir("reconnect");
    let store = RecordingStore::default();
    let hub = Hub::new(settings_path(&dir), &store);
    hub.connect("deepgram-nova-3", SECRET).unwrap();
    let s = hub.connect("deepgram-nova-3", "dg-other-key-5678").unwrap();
    assert_eq!(s.key_tail.as_deref(), Some("5678"));
    let saved = load(&settings_path(&dir)).settings;
    assert_eq!(
        saved
            .providers
            .iter()
            .filter(|p| p.id == "deepgram-nova-3")
            .count(),
        1
    );
}

#[test]
fn short_keys_show_no_tail_and_bad_keys_are_rejected() {
    let dir = temp_dir("shortkey");
    let store = RecordingStore::default();
    let hub = Hub::new(settings_path(&dir), &store);
    let s = hub.connect("anthropic", "abc12").unwrap();
    assert_eq!(s.key_tail, None);
    for bad in ["", "   ", "has space inside", "line\nbreak"] {
        let err = hub.connect("anthropic", bad).unwrap_err();
        assert!(!err.contains("inside"), "{err}");
    }
}

#[test]
fn disconnect_removes_key_entry_and_chain_reference() {
    let dir = temp_dir("disconnect");
    let path = settings_path(&dir);
    let store = RecordingStore::default();
    let hub = Hub::new(path.clone(), &store);
    hub.connect("elevenlabs-tts", SECRET).unwrap();
    let mut s = load(&path).settings;
    s.chains.tts.push("elevenlabs-tts".into());
    save(&path, &s, false).unwrap();

    hub.disconnect("elevenlabs-tts").unwrap();
    assert_eq!(
        *store.deletes.lock().unwrap(),
        vec!["provider:elevenlabs-tts".to_owned()]
    );
    assert_eq!(store.get_key("provider:elevenlabs-tts").unwrap(), None);
    let after = load(&path).settings;
    assert!(!after.providers.iter().any(|p| p.id == "elevenlabs-tts"));
    assert!(!after.chains.tts.contains(&"elevenlabs-tts".to_owned()));
    // Disconnecting the built-in engine is refused.
    assert!(hub.disconnect("whisper-local").is_err());
}

#[test]
fn key_store_failure_never_echoes_the_key_and_writes_no_settings() {
    let dir = temp_dir("storefail");
    let path = settings_path(&dir);
    let store = RecordingStore {
        fail_writes: true,
        ..Default::default()
    };
    let hub = Hub::new(path.clone(), &store);
    let err = hub.connect("anthropic", SECRET).unwrap_err();
    assert_no_secret(&err);
    assert!(!path.exists());

    let err = hub
        .save_custom(
            &form("Mine", "https://api.example.com/v1", "m", "bearer"),
            SECRET,
        )
        .unwrap_err();
    assert_no_secret(&err);
}

#[test]
fn custom_provider_save_edit_delete() {
    let dir = temp_dir("custom");
    let path = settings_path(&dir);
    let store = RecordingStore::default();
    let hub = Hub::new(path.clone(), &store);

    let created = hub
        .save_custom(
            &form(
                "My Server",
                "https://api.example.com/v1/",
                "gpt-5-nano",
                "bearer",
            ),
            SECRET,
        )
        .unwrap();
    assert_eq!(created.id, "custom-my-server");
    assert!(created.custom && created.connected);
    assert_eq!(
        created.base_url.as_deref(),
        Some("https://api.example.com/v1")
    );
    assert_eq!(created.name.as_deref(), Some("My Server"));
    assert_no_secret(&serde_json::to_string(&created).unwrap());

    // Same name again gets its own id.
    let second = hub
        .save_custom(
            &form("My Server", "https://other.example.com", "m", "bearer"),
            "another-key-0000",
        )
        .unwrap();
    assert_eq!(second.id, "custom-my-server-2");

    let text = fs::read_to_string(&path).unwrap();
    assert_no_secret(&text);
    let v: Value = serde_json::from_str(&text).unwrap();
    let entry = v["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "custom-my-server")
        .unwrap();
    let mut keys: Vec<&str> = entry
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "adapter",
            "authStyle",
            "baseUrl",
            "id",
            "keyRef",
            "kind",
            "model",
            "name"
        ]
    );
    assert_eq!(entry["adapter"], "openai_compatible");
    assert_eq!(entry["authStyle"], "bearer");

    // Edit without a new key keeps the stored key.
    let mut edit = form(
        "My Server",
        "https://api.example.com/v2",
        "gpt-5-mini",
        "x-api-key",
    );
    edit.id = Some("custom-my-server".into());
    let edited = hub.save_custom(&edit, "").unwrap();
    assert!(edited.connected);
    assert_eq!(edited.model, "gpt-5-mini");
    assert_eq!(edited.auth_style.as_deref(), Some("x-api-key"));
    assert_eq!(
        store
            .get_key("provider:custom-my-server")
            .unwrap()
            .as_deref(),
        Some(SECRET)
    );

    // Editing a catalog or unknown id through the custom form is refused.
    let mut bad = form("x", "https://a.b", "m", "bearer");
    bad.id = Some("sarvam-saaras".into());
    assert!(hub.save_custom(&bad, SECRET).is_err());
    bad.id = Some("custom-missing".into());
    assert!(hub.save_custom(&bad, SECRET).is_err());

    hub.delete_custom("custom-my-server").unwrap();
    assert_eq!(store.get_key("provider:custom-my-server").unwrap(), None);
    let after = load(&path).settings;
    assert!(!after.providers.iter().any(|p| p.id == "custom-my-server"));
    assert!(after.providers.iter().any(|p| p.id == "custom-my-server-2"));
    assert!(hub.delete_custom("whisper-local").is_err());
}

#[test]
fn custom_provider_needs_a_key_unless_auth_is_none() {
    let dir = temp_dir("customkey");
    let store = RecordingStore::default();
    let hub = Hub::new(settings_path(&dir), &store);
    assert!(
        hub.save_custom(&form("Cloud", "https://api.example.com", "m", "bearer"), "")
            .is_err()
    );
    let local = hub
        .save_custom(
            &form("Ollama", "http://localhost:11434/v1", "llama3.2", "none"),
            "ignored-key-1234",
        )
        .unwrap();
    assert!(local.connected);
    assert_eq!(local.key, KeyState::NotNeeded);
    assert!(store.writes.lock().unwrap().is_empty());
    let saved = load(&settings_path(&dir)).settings;
    let p = saved.providers.iter().find(|p| p.id == local.id).unwrap();
    assert_eq!(p.key_ref, None);
}

// ---------- test connection seam ----------

struct FakeProbe;
impl HttpProbe for FakeProbe {
    fn available(&self) -> bool {
        true
    }
    fn probe(&self, target: &ProbeTarget<'_>, key: Option<&str>) -> TestResult {
        assert_eq!(target.base_url, "https://api.example.com/v1");
        assert_eq!(key, Some(SECRET));
        TestResult::Ok { latency_ms: 42 }
    }
}

#[test]
fn test_connection_is_not_available_without_an_http_client() {
    let dir = temp_dir("probe");
    let store = RecordingStore::default();
    let hub = Hub::new(settings_path(&dir), &store);
    let p = hub
        .save_custom(
            &form("Probe", "https://api.example.com/v1", "m", "bearer"),
            SECRET,
        )
        .unwrap();
    let r = hub.test(&p.id, &NoHttpClient).unwrap();
    assert!(matches!(r, TestResult::NotAvailable { .. }));
    assert_eq!(serde_json::to_value(&r).unwrap()["result"], "not_available");
    let r = hub.test(&p.id, &FakeProbe).unwrap();
    assert_eq!(r, TestResult::Ok { latency_ms: 42 });
    assert_no_secret(&serde_json::to_string(&r).unwrap());
    assert!(hub.test("custom-missing", &NoHttpClient).is_err());
    assert_eq!(
        serde_json::to_value(TestResult::Ok { latency_ms: 7 }).unwrap(),
        serde_json::json!({"result": "ok", "latencyMs": 7})
    );
}

#[test]
fn catalog_providers_without_a_documented_free_check_are_not_tested() {
    let dir = temp_dir("probe-catalog");
    let store = RecordingStore::default();
    let hub = Hub::new(settings_path(&dir), &store);
    hub.connect("sarvam-saaras", SECRET).unwrap();
    assert_eq!(
        hub.test("sarvam-saaras", &FakeProbe).unwrap(),
        TestResult::NotAvailable {
            reason: REASON_NO_FREE_CHECK
        }
    );
}

// ---------- real HTTP client against a local canned server (no internet) ----------

/// Serves one canned response per connection; records each request head.
fn serve(responses: Vec<&'static str>, delay: Duration) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for (stream, resp) in listener.incoming().zip(responses) {
            let Ok(mut stream) = stream else { return };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            log.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf[..n]).into_owned());
            std::thread::sleep(delay);
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    (format!("http://127.0.0.1:{}/v1", addr.port()), seen)
}

const OK_200: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"data\":[]}";
const R_401: &str = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const R_404: &str = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

fn probe_once(base: &str, auth: &AuthStyle, key: Option<&str>, probe: &UreqProbe) -> TestResult {
    let target = ProbeTarget {
        capability: Capability::Llm,
        base_url: base,
        auth,
        model: "m",
    };
    probe.probe(&target, key)
}

fn quick() -> UreqProbe {
    UreqProbe::with_timeouts(Duration::from_millis(500), Duration::from_millis(800))
}

#[test]
fn http_probe_ok_sends_key_only_in_the_auth_header() {
    let (base, seen) = serve(vec![OK_200], Duration::ZERO);
    let r = probe_once(&base, &AuthStyle::Bearer, Some(SECRET), &quick());
    assert!(matches!(r, TestResult::Ok { .. }), "{r:?}");
    assert_no_secret(&serde_json::to_string(&r).unwrap());
    let head = seen.lock().unwrap()[0].clone();
    let first_line = head.lines().next().unwrap();
    assert_eq!(first_line, "GET /v1/models HTTP/1.1");
    assert!(!first_line.contains(SECRET), "key in URL");
    let auth_lines: Vec<&str> = head.lines().filter(|l| l.contains(SECRET)).collect();
    assert_eq!(auth_lines.len(), 1);
    assert!(
        auth_lines[0]
            .to_lowercase()
            .starts_with("authorization: bearer ")
    );
}

#[test]
fn http_probe_custom_header_and_no_key_styles() {
    let (base, seen) = serve(vec![OK_200, OK_200], Duration::ZERO);
    let h = AuthStyle::Header("X-API-Key-ID".into());
    assert!(matches!(
        probe_once(&base, &h, Some(SECRET), &quick()),
        TestResult::Ok { .. }
    ));
    assert!(matches!(
        probe_once(&base, &AuthStyle::None, None, &quick()),
        TestResult::Ok { .. }
    ));
    let heads = seen.lock().unwrap();
    assert!(
        heads[0]
            .to_lowercase()
            .contains(&format!("x-api-key-id: {}", SECRET.to_lowercase()))
    );
    assert!(!heads[1].to_lowercase().contains("authorization"));
}

#[test]
fn http_probe_classifies_401_404_and_timeout() {
    let (base, _) = serve(vec![R_401], Duration::ZERO);
    assert_eq!(
        probe_once(&base, &AuthStyle::XApiKey, Some(SECRET), &quick()),
        TestResult::KeyRejected
    );
    let (base, _) = serve(vec![R_404], Duration::ZERO);
    assert_eq!(
        probe_once(&base, &AuthStyle::Token, Some(SECRET), &quick()),
        TestResult::NotFound
    );
    let (base, _) = serve(vec![OK_200], Duration::from_millis(2000));
    assert_eq!(
        probe_once(&base, &AuthStyle::Bearer, Some(SECRET), &quick()),
        TestResult::Timeout
    );
}

#[test]
fn http_probe_never_follows_a_redirect_to_another_host() {
    let (other, other_seen) = serve(vec![OK_200], Duration::ZERO);
    let location: &'static str = Box::leak(
        format!(
            "HTTP/1.1 302 Found\r\nLocation: {}/models\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            other.replace("127.0.0.1", "localhost")
        )
        .into_boxed_str(),
    );
    let (base, _) = serve(vec![location], Duration::ZERO);
    let r = probe_once(&base, &AuthStyle::Bearer, Some(SECRET), &quick());
    assert_eq!(r, TestResult::Other { status: 302 });
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        other_seen.lock().unwrap().is_empty(),
        "redirect was followed"
    );
}

#[test]
fn http_probe_unreachable_and_refuses_plain_http_off_this_mac() {
    // Bind then drop to get a port with nothing listening.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let base = format!("http://127.0.0.1:{port}/v1");
    let r = probe_once(&base, &AuthStyle::Bearer, Some(SECRET), &quick());
    assert_eq!(r, TestResult::Unreachable);
    assert_no_secret(&format!("{r:?}"));
    let r = probe_once(
        "http://example.com/v1",
        &AuthStyle::Bearer,
        Some(SECRET),
        &quick(),
    );
    assert_eq!(
        r,
        TestResult::NotAvailable {
            reason: REASON_NOT_HTTPS
        }
    );
}

#[test]
fn hub_test_runs_the_real_client_against_a_custom_provider() {
    let (base, _) = serve(vec![R_401], Duration::ZERO);
    let dir = temp_dir("probe-hub");
    let store = RecordingStore::default();
    let hub = Hub::new(settings_path(&dir), &store);
    let p = hub
        .save_custom(&form("Local", &base, "m", "bearer"), SECRET)
        .unwrap();
    let r = hub.test(&p.id, &quick()).unwrap();
    assert_eq!(r, TestResult::KeyRejected);
}

// ---------- settings compatibility ----------

#[test]
fn older_settings_without_new_fields_load_and_round_trip_unchanged() {
    let dir = temp_dir("compat");
    let path = settings_path(&dir);
    let old = r#"{
  "schemaVersion": 1,
  "providers": [
    {"id": "whisper-local", "kind": "stt", "adapter": "whisper_local", "model": "base.en"},
    {"id": "haiku", "kind": "llm", "adapter": "anthropic", "model": "m", "keyRef": "anthropic"}
  ]
}"#;
    fs::write(&path, old).unwrap();
    let r = load(&path);
    assert_eq!(r.dropped_entries, 0);
    assert_eq!(r.settings.providers.len(), 2);
    assert_eq!(r.settings.providers[1].name, None);
    assert_eq!(r.settings.providers[1].auth_style, None);
    let out: Value = serde_json::from_str(&settings::export(&r.settings).unwrap()).unwrap();
    for p in out["providers"].as_array().unwrap() {
        assert!(
            p.get("authStyle").is_none() && p.get("name").is_none(),
            "{p}"
        );
    }
    // The hub reads the old file and reports the existing entry.
    let store = MemoryKeyStore::default();
    let hub = Hub::new(path.clone(), &store);
    let all = hub.status().unwrap();
    assert!(all.iter().any(|s| s.id == "haiku" && !s.connected));
    let _ = Settings::default();
}

// ---------- dev preview copy used by the browser preview of the UI ----------

/// The UI bundles `catalog-preview.json` for the dev browser preview only. It must
/// match the Rust catalog. Regenerate with `UPDATE_PROVIDER_PREVIEW=1 cargo test`.
#[test]
fn ui_preview_catalog_matches_the_rust_catalog() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../src/settings/providers/catalog-preview.json");
    let want = serde_json::to_value(CATALOG).unwrap();
    if std::env::var("UPDATE_PROVIDER_PREVIEW").is_ok() {
        fs::write(&path, serde_json::to_string_pretty(&want).unwrap() + "\n").unwrap();
    }
    let have: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        have, want,
        "run with UPDATE_PROVIDER_PREVIEW=1 to refresh the UI copy"
    );
}
