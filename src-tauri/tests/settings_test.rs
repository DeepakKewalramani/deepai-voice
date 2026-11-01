use deepai_voice_lib::settings::keys::{
    KeyStore, MemoryKeyStore, SERVICE, env_var_name, resolve_key,
};
use deepai_voice_lib::settings::{SCHEMA_VERSION, Settings, export, load, save, settings_path};
use std::fs;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("deepai-settings-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn defaults_match_adr_shape() {
    let s = Settings::default();
    assert_eq!(s.schema_version, 1);
    assert_eq!(s.active_style, "clean");
    assert_eq!(s.max_record_seconds, 120);
    assert_eq!(s.timeouts.total_deadline_ms, 10000);
    let json = export(&s).unwrap();
    for field in [
        "schemaVersion",
        "hotkeys",
        "providers",
        "chains",
        "styles",
        "activeStyle",
        "timeouts",
        "privacy",
        "maxRecordSeconds",
    ] {
        assert!(json.contains(field), "missing {field}");
    }
}

#[test]
fn missing_file_loads_defaults() {
    let dir = temp_dir("missing");
    let r = load(&settings_path(&dir));
    assert_eq!(r.settings, Settings::default());
    assert!(r.backup.is_none() && !r.read_only);
}

#[test]
fn save_then_load_round_trips() {
    let dir = temp_dir("roundtrip");
    let path = settings_path(&dir);
    let mut s = Settings {
        max_record_seconds: 60,
        ..Settings::default()
    };
    s.privacy
        .cloud_acknowledged
        .insert("sarvam-1".into(), "2026-10-31".into());
    save(&path, &s, false).unwrap();
    let r = load(&path);
    assert_eq!(r.settings, s);
    assert!(r.migrated_from.is_none());
}

#[test]
fn v0_migrates_to_v1_and_drops_raw_keys() {
    let dir = temp_dir("migrate");
    let path = settings_path(&dir);
    fs::write(&path, r#"{
        "hotkeys": {"dictate": "Cmd+Shift+D"},
        "providers": [{"id": "haiku", "kind": "llm", "adapter": "anthropic", "model": "m", "apiKey": "test-secret"}],
        "maxRecordSeconds": 30
    }"#).unwrap();
    let r = load(&path);
    assert_eq!(r.migrated_from, Some(0));
    assert_eq!(r.settings.schema_version, SCHEMA_VERSION);
    assert_eq!(r.settings.hotkeys.dictate, "Cmd+Shift+D");
    assert_eq!(r.settings.max_record_seconds, 30);
    assert_eq!(r.settings.providers[0].key_ref.as_deref(), Some("haiku"));
    save(&path, &r.settings, r.read_only).unwrap();
    assert!(!fs::read_to_string(&path).unwrap().contains("test-secret"));
}

#[test]
fn corrupt_file_is_backed_up_and_defaults_load() {
    let dir = temp_dir("corrupt");
    let path = settings_path(&dir);
    fs::write(&path, "{ not json").unwrap();
    let r = load(&path);
    assert_eq!(r.settings, Settings::default());
    let backup = r.backup.expect("backup created");
    assert_eq!(fs::read_to_string(backup).unwrap(), "{ not json");
}

#[test]
fn wrong_types_are_backed_up_not_panicking() {
    let dir = temp_dir("wrongtype");
    let path = settings_path(&dir);
    fs::write(&path, r#"{"schemaVersion": 1, "timeouts": "oops"}"#).unwrap();
    let r = load(&path);
    assert_eq!(r.settings, Settings::default());
    assert!(r.backup.is_some());
}

#[test]
fn newer_version_is_read_only_and_not_overwritten() {
    let dir = temp_dir("newer");
    let path = settings_path(&dir);
    let original = r#"{"schemaVersion": 99, "maxRecordSeconds": 45, "futureField": true}"#;
    fs::write(&path, original).unwrap();
    let r = load(&path);
    assert!(r.read_only);
    assert_eq!(r.settings.max_record_seconds, 45);
    assert!(save(&path, &r.settings, r.read_only).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
}

#[test]
fn key_never_in_settings_file() {
    let dir = temp_dir("secret");
    let path = settings_path(&dir);
    let store = MemoryKeyStore::default();
    store.set_key("anthropic", "test-secret").unwrap();
    let mut s = Settings::default();
    s.providers.push(deepai_voice_lib::settings::Provider {
        id: "haiku".into(),
        kind: "llm".into(),
        adapter: "anthropic".into(),
        model: "claude-haiku-5-5".into(),
        base_url: None,
        key_ref: Some("anthropic".into()),
        name: None,
        auth_style: None,
    });
    save(&path, &s, false).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(!text.contains("test-secret"));
    assert!(text.contains("\"keyRef\": \"anthropic\""));
    assert!(!export(&s).unwrap().contains("test-secret"));
}

#[test]
fn key_store_crud() {
    let store = MemoryKeyStore::default();
    assert!(!store.has_key("sarvam").unwrap());
    store.set_key("sarvam", "abc").unwrap();
    assert!(store.has_key("sarvam").unwrap());
    assert_eq!(store.get_key("sarvam").unwrap().as_deref(), Some("abc"));
    store.delete_key("sarvam").unwrap();
    assert_eq!(store.get_key("sarvam").unwrap(), None);
    assert_eq!(SERVICE, "com.deepakkewalramani.deepaivoice");
}

#[test]
fn env_var_overrides_store() {
    let store = MemoryKeyStore::default();
    store.set_key("anthropic", "from-keychain").unwrap();
    assert_eq!(env_var_name("anthropic"), "ANTHROPIC_API_KEY");
    assert_eq!(env_var_name("custom-1"), "CUSTOM_1_API_KEY");
    let env = |n: &str| (n == "ANTHROPIC_API_KEY").then(|| "from-env".to_string());
    assert_eq!(
        resolve_key(&store, "anthropic", env).unwrap().as_deref(),
        Some("from-env")
    );
    assert_eq!(
        resolve_key(&store, "anthropic", |_| None)
            .unwrap()
            .as_deref(),
        Some("from-keychain")
    );
    assert_eq!(
        resolve_key(&store, "anthropic", |_| Some(String::new()))
            .unwrap()
            .as_deref(),
        Some("from-keychain")
    );
}

#[test]
fn unreadable_path_is_read_only_not_defaults_overwrite() {
    let dir = temp_dir("isdir");
    let path = settings_path(&dir);
    fs::create_dir_all(&path).unwrap(); // path is a directory
    let r = load(&path);
    assert!(r.read_only);
    assert!(save(&path, &r.settings, r.read_only).is_err());
}

#[test]
fn invalid_utf8_is_read_only() {
    let dir = temp_dir("utf8");
    let path = settings_path(&dir);
    fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
    let r = load(&path);
    assert!(r.read_only);
    assert_eq!(fs::read(&path).unwrap(), vec![0xff, 0xfe, 0x00]);
}

#[test]
fn two_backups_do_not_clobber() {
    let dir = temp_dir("twobak");
    let path = settings_path(&dir);
    fs::write(&path, "bad one").unwrap();
    let a = load(&path).backup.unwrap();
    fs::write(&path, "bad two").unwrap();
    let b = load(&path).backup.unwrap();
    assert_ne!(a, b);
    assert_eq!(fs::read_to_string(a).unwrap(), "bad one");
    assert_eq!(fs::read_to_string(b).unwrap(), "bad two");
}

#[test]
fn bad_list_entry_dropped_rest_kept() {
    let dir = temp_dir("badentry");
    let path = settings_path(&dir);
    fs::write(&path, r#"{
        "schemaVersion": 1,
        "providers": [
            {"id": "a", "kind": "stt", "adapter": "whisper_local"},
            {"id": 5},
            {"id": "b", "kind": "llm", "adapter": "anthropic", "keyRef": "anthropic"}
        ],
        "styles": [{"id": "x"}, {"id": "clean", "name": "C", "systemPrompt": "s", "userTemplate": "t"}],
        "maxRecordSeconds": 33
    }"#).unwrap();
    let r = load(&path);
    assert!(r.backup.is_none() && !r.read_only);
    assert_eq!(r.settings.max_record_seconds, 33);
    let ids: Vec<_> = r.settings.providers.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
    assert_eq!(r.settings.styles.len(), 1);
    assert_eq!(r.dropped_entries, 2);
}

#[test]
fn migration_reports_dropped_key_ids() {
    let dir = temp_dir("dropped");
    let path = settings_path(&dir);
    fs::write(
        &path,
        r#"{"providers": [
        {"id": "haiku", "kind": "llm", "adapter": "anthropic", "apiKey": "test-secret"},
        {"id": "whisper-local", "kind": "stt", "adapter": "whisper_local"}
    ]}"#,
    )
    .unwrap();
    let r = load(&path);
    assert_eq!(r.keys_dropped, vec!["haiku".to_string()]);
    assert!(!format!("{r:?}").contains("test-secret"));
}
