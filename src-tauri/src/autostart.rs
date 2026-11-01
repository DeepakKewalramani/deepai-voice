//! Launch at login through a per-user LaunchAgent, with no extra crate.
//!
//! The agent runs `/usr/bin/open -a <bundle>` at login. It is only written when the app runs
//! from a `.app` bundle, so a dev or test binary is never registered. On the first launch of a
//! bundled app it is turned on once; after that the user's choice is kept (marker file
//! `autostart.json` in the app data dir).

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const LABEL: &str = "com.deepakkewalramani.deepaivoice";
pub const MARKER_FILE: &str = "autostart.json";

/// Error shown when the app is not running from an installed `.app` bundle.
pub const NOT_BUNDLED: &str =
    "Launch at login works only in the installed app, not in a development build.";

/// The `.app` bundle that contains `exe`: the path up to and including the first component
/// ending in `.app`. None when `exe` is not inside a bundle.
pub fn bundle_path(exe: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    let mut components = exe.components();
    for c in components.by_ref() {
        out.push(c.as_os_str());
        if let Component::Normal(name) = c
            && name
                .to_str()
                .is_some_and(|n| n.ends_with(".app") && n.len() > 4)
        {
            // The bundle must contain the executable, not be it.
            return components.next().map(|_| out);
        }
    }
    None
}

/// `~/Library/LaunchAgents/<label>.plist`.
pub fn plist_path(home: &Path) -> PathBuf {
    home.join("Library")
        .join("LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn bundle_string(bundle: &Path) -> String {
    format!("<string>{}</string>", xml_escape(&bundle.to_string_lossy()))
}

/// The LaunchAgent plist that opens `bundle` at login.
pub fn plist_xml(bundle: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/bin/open</string>
    <string>-a</string>
    {}
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>ProcessType</key>
  <string>Interactive</string>
</dict>
</plist>
"#,
        bundle_string(bundle)
    )
}

/// True when the plist at `plist` exists and opens `bundle`.
pub fn is_enabled_at(plist: &Path, bundle: Option<&Path>) -> bool {
    let Some(bundle) = bundle else { return false };
    std::fs::read_to_string(plist)
        .map(|s| s.contains(&bundle_string(bundle)))
        .unwrap_or(false)
}

/// Writes the plist atomically (temp file, then rename).
pub fn enable_at(plist: &Path, bundle: Option<&Path>) -> Result<(), String> {
    let bundle = bundle.ok_or(NOT_BUNDLED)?;
    let dir = plist.parent().ok_or("invalid LaunchAgents path")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {dir:?}: {e}"))?;
    let tmp = dir.join(format!(".{LABEL}.plist.tmp"));
    std::fs::write(&tmp, plist_xml(bundle)).map_err(|e| format!("could not write: {e}"))?;
    std::fs::rename(&tmp, plist).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("could not save: {e}")
    })
}

/// Removes the plist. A missing file is fine.
pub fn disable_at(plist: &Path) -> Result<(), String> {
    match std::fs::remove_file(plist) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("could not remove: {e}")),
    }
}

/// Whether the launch at login choice has been made (by the first run or by the user).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    #[serde(default)]
    pub decided: bool,
}

pub fn load_marker(path: &Path) -> Marker {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_marker(path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(&Marker { decided: true }).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// First run default: turn it on once, only for a bundled app that has not decided yet.
pub fn should_enable_on_launch(marker: Marker, bundled: bool) -> bool {
    bundled && !marker.decided
}

/// Applies the first run default. A dev build changes nothing and writes no marker, so the
/// first launch of the installed app still turns it on.
pub fn first_launch_at(marker_path: &Path, plist: &Path, bundle: Option<&Path>) {
    if !should_enable_on_launch(load_marker(marker_path), bundle.is_some()) {
        return;
    }
    match enable_at(plist, bundle) {
        Ok(()) => {
            let _ = save_marker(marker_path);
        }
        Err(e) => tracing::warn!("launch at login not enabled: {e}"),
    }
}

/// Saves the user's choice and records that it was decided.
pub fn set_at(
    marker_path: &Path,
    plist: &Path,
    bundle: Option<&Path>,
    enabled: bool,
) -> Result<(), String> {
    if enabled {
        enable_at(plist, bundle)?;
    } else {
        disable_at(plist)?;
    }
    let _ = save_marker(marker_path);
    Ok(())
}

fn current_bundle() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|e| bundle_path(&e))
}

fn home_plist() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| plist_path(Path::new(&h)))
}

fn marker_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    use tauri::Manager;
    app.path().app_data_dir().ok().map(|d| d.join(MARKER_FILE))
}

/// Called once at startup.
pub fn on_launch<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if !cfg!(target_os = "macos") {
        return;
    }
    if let (Some(marker), Some(plist)) = (marker_path(app), home_plist()) {
        first_launch_at(&marker, &plist, current_bundle().as_deref());
    }
}

/// True when the LaunchAgent exists and opens this app bundle.
#[tauri::command]
pub fn autostart_status() -> bool {
    cfg!(target_os = "macos")
        && home_plist().is_some_and(|p| is_enabled_at(&p, current_bundle().as_deref()))
}

/// Turns launch at login on or off. Takes effect at the next login.
#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("Launch at login is only available on macOS.".into());
    }
    let plist = home_plist().ok_or("home folder not found")?;
    let marker = marker_path(&app).ok_or("app data folder not found")?;
    set_at(&marker, &plist, current_bundle().as_deref(), enabled)?;
    if !enabled {
        bootout(&plist);
    }
    Ok(())
}

/// Best effort: unloads the agent from the current session. Errors are ignored.
fn bootout(plist: &Path) {
    let uid = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()));
    if let Some(uid) = uid {
        let _ = std::process::Command::new("/bin/launchctl")
            .arg("bootout")
            .arg(format!("gui/{uid}"))
            .arg(plist)
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("deepai-autostart-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn bundle_path_is_the_first_app_component() {
        assert_eq!(
            bundle_path(Path::new(
                "/Applications/DeepAI Voice.app/Contents/MacOS/deepai-voice"
            )),
            Some(PathBuf::from("/Applications/DeepAI Voice.app"))
        );
        assert_eq!(
            bundle_path(Path::new(
                "/A/Outer.app/Contents/Inner.app/Contents/MacOS/x"
            )),
            Some(PathBuf::from("/A/Outer.app"))
        );
    }

    #[test]
    fn bundle_path_is_none_outside_a_bundle() {
        for exe in [
            "/Users/me/deepAi/src-tauri/target/debug/deepai-voice",
            "/Users/me/.cache/target/debug/deps/deepai_voice-abc123",
            "/Applications/DeepAI Voice.app",
            "/x/.app/Contents/MacOS/y",
            "/x/myapp/Contents/MacOS/y",
        ] {
            assert_eq!(bundle_path(Path::new(exe)), None, "{exe}");
        }
        let test_exe = std::env::current_exe().unwrap();
        assert_eq!(bundle_path(&test_exe), None);
    }

    #[test]
    fn plist_path_is_under_launch_agents() {
        assert_eq!(
            plist_path(Path::new("/Users/me")),
            PathBuf::from("/Users/me/Library/LaunchAgents/com.deepakkewalramani.deepaivoice.plist")
        );
    }

    #[test]
    fn plist_has_label_open_args_run_at_load_and_no_keep_alive() {
        let xml = plist_xml(Path::new("/Applications/DeepAI Voice.app"));
        assert!(xml.contains("<key>Label</key>\n  <string>com.deepakkewalramani.deepaivoice</string>"));
        assert!(xml.contains(
            "<string>/usr/bin/open</string>\n    <string>-a</string>\n    <string>/Applications/DeepAI Voice.app</string>"
        ));
        assert!(xml.contains("<key>RunAtLoad</key>\n  <true/>"));
        assert!(xml.contains("<key>ProcessType</key>\n  <string>Interactive</string>"));
        assert!(!xml.contains("KeepAlive"));
    }

    #[test]
    fn plist_escapes_the_path() {
        let xml = plist_xml(Path::new("/Apps/A&B <\"x'>.app"));
        assert!(xml.contains("<string>/Apps/A&amp;B &lt;&quot;x&apos;&gt;.app</string>"));
        assert!(!xml.contains("A&B"));
    }

    #[test]
    fn enable_refuses_without_a_bundle_and_writes_nothing() {
        let d = temp_dir("nobundle");
        let plist = plist_path(&d);
        assert_eq!(enable_at(&plist, None), Err(NOT_BUNDLED.to_string()));
        assert!(!plist.exists());
        assert!(!is_enabled_at(&plist, None));
    }

    #[test]
    fn enable_status_and_disable_round_trip() {
        let d = temp_dir("roundtrip");
        let plist = plist_path(&d);
        let bundle = Path::new("/Applications/DeepAI Voice.app");
        enable_at(&plist, Some(bundle)).unwrap();
        assert!(is_enabled_at(&plist, Some(bundle)));
        assert!(!is_enabled_at(&plist, Some(Path::new("/Other.app"))));
        assert!(!plist.with_file_name(format!(".{LABEL}.plist.tmp")).exists());
        disable_at(&plist).unwrap();
        assert!(!plist.exists());
        disable_at(&plist).unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn first_run_decision() {
        let undecided = Marker::default();
        let decided = Marker { decided: true };
        assert!(should_enable_on_launch(undecided, true));
        assert!(!should_enable_on_launch(undecided, false));
        assert!(!should_enable_on_launch(decided, true));
    }

    #[test]
    fn first_launch_enables_once_and_user_off_stays_off() {
        let d = temp_dir("firstrun");
        let plist = plist_path(&d);
        let marker = d.join("data").join(MARKER_FILE);
        let bundle = Some(Path::new("/Applications/DeepAI Voice.app"));

        first_launch_at(&marker, &plist, bundle);
        assert!(is_enabled_at(&plist, bundle));
        assert_eq!(load_marker(&marker), Marker { decided: true });

        set_at(&marker, &plist, bundle, false).unwrap();
        first_launch_at(&marker, &plist, bundle);
        assert!(!plist.exists(), "user choice must survive the next launch");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn dev_build_launch_changes_nothing() {
        let d = temp_dir("devrun");
        let plist = plist_path(&d);
        let marker = d.join(MARKER_FILE);
        first_launch_at(&marker, &plist, None);
        assert!(!plist.exists());
        assert!(!marker.exists());
        assert!(set_at(&marker, &plist, None, true).is_err());
        assert!(!plist.exists());
    }
}
