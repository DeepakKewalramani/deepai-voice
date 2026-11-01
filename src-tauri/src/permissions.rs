//! macOS privacy permissions shown in Settings > Permissions.
//!
//! Status reads go through the existing seams (paste, hotkey, audio). Opening a pane runs
//! `open` with a fixed URL picked from `PermissionKind`; user input only selects the kind and
//! never reaches a command line. Off macOS every status is `Unknown` and the commands return
//! an error.

use serde::Serialize;

/// A permission the app needs on macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionKind {
    Accessibility,
    InputMonitoring,
    Microphone,
    /// Optional: on-device live typing. Without it dictation uses Whisper.
    SpeechRecognition,
}

impl PermissionKind {
    pub const ALL: [PermissionKind; 4] = [
        PermissionKind::Accessibility,
        PermissionKind::InputMonitoring,
        PermissionKind::Microphone,
        PermissionKind::SpeechRecognition,
    ];

    /// Parses the id the frontend sends. Anything else is rejected.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "accessibility" => Some(Self::Accessibility),
            "inputMonitoring" => Some(Self::InputMonitoring),
            "microphone" => Some(Self::Microphone),
            "speechRecognition" => Some(Self::SpeechRecognition),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility",
            Self::InputMonitoring => "inputMonitoring",
            Self::Microphone => "microphone",
            Self::SpeechRecognition => "speechRecognition",
        }
    }

    /// Whether the app can ask macOS to show the access prompt for this kind.
    pub fn can_prompt(self) -> bool {
        matches!(self, Self::Microphone | Self::SpeechRecognition)
    }
}

/// The System Settings pane for a permission.
pub fn pane_url(kind: PermissionKind) -> &'static str {
    match kind {
        PermissionKind::Accessibility => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        PermissionKind::InputMonitoring => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
        PermissionKind::Microphone => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        PermissionKind::SpeechRecognition => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_SpeechRecognition"
        }
    }
}

/// One permission's state as shown in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionState {
    Granted,
    Needed,
    Unknown,
}

/// Status of every permission, sent to the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionsStatus {
    pub accessibility: PermissionState,
    pub input_monitoring: PermissionState,
    pub microphone: PermissionState,
    /// True when macOS has not asked yet, so "Request access" can show the prompt.
    pub microphone_can_request: bool,
    /// Optional permission; never counts as missing in the UI.
    pub speech_recognition: PermissionState,
    pub speech_can_request: bool,
}

impl PermissionsStatus {
    pub const UNKNOWN: PermissionsStatus = PermissionsStatus {
        accessibility: PermissionState::Unknown,
        input_monitoring: PermissionState::Unknown,
        microphone: PermissionState::Unknown,
        microphone_can_request: false,
        speech_recognition: PermissionState::Unknown,
        speech_can_request: false,
    };
}

fn from_bool(granted: bool) -> PermissionState {
    if granted {
        PermissionState::Granted
    } else {
        PermissionState::Needed
    }
}

/// Maps a granted / denied / not asked status to (state, can_request).
fn from_tri(granted: bool, undetermined: bool) -> (PermissionState, bool) {
    (from_bool(granted), !granted && undetermined)
}

#[cfg(target_os = "macos")]
fn read_status() -> PermissionsStatus {
    use crate::audio::{MicPermissionStatus, check_microphone_permission};
    use crate::stt::apple_speech::{
        SfSpeechPermission, SpeechPermissionProvider, SpeechPermissionStatus,
    };

    let mic = check_microphone_permission();
    let (microphone, microphone_can_request) = from_tri(
        mic == MicPermissionStatus::Granted,
        mic == MicPermissionStatus::Undetermined,
    );
    let speech = SfSpeechPermission.status();
    let (speech_recognition, speech_can_request) = from_tri(
        speech == SpeechPermissionStatus::Granted,
        speech == SpeechPermissionStatus::Undetermined,
    );
    PermissionsStatus {
        accessibility: from_bool(crate::paste::macos::is_accessibility_trusted()),
        input_monitoring: from_bool(crate::hotkey::macos::has_permission()),
        microphone,
        microphone_can_request,
        speech_recognition,
        speech_can_request,
    }
}

#[cfg(not(target_os = "macos"))]
fn read_status() -> PermissionsStatus {
    PermissionsStatus::UNKNOWN
}

/// Current status of every permission. Never prompts.
#[tauri::command]
pub fn permissions_status() -> PermissionsStatus {
    read_status()
}

/// Opens the System Settings pane for `kind`.
#[tauri::command]
pub fn open_permission_pane(kind: String) -> Result<(), String> {
    let kind = PermissionKind::from_id(&kind).ok_or("unknown permission")?;
    open_url(pane_url(kind))
}

#[cfg(target_os = "macos")]
fn open_url(url: &'static str) -> Result<(), String> {
    let mut child = std::process::Command::new("/usr/bin/open")
        .arg(url)
        .spawn()
        .map_err(|e| format!("could not open System Settings: {e}"))?;
    // Reap the short-lived `open` process off the calling thread.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn open_url(_url: &'static str) -> Result<(), String> {
    Err("System Settings is only available on macOS".into())
}

/// Asks macOS to show the microphone or speech recognition prompt. macOS shows it only
/// while the permission is not decided; the UI polls for the result.
#[tauri::command]
pub fn request_permission(kind: String) -> Result<(), String> {
    let kind = PermissionKind::from_id(&kind).ok_or("unknown permission")?;
    if !kind.can_prompt() {
        return Err("this permission is granted in System Settings".into());
    }
    prompt(kind)
}

/// True when this process runs from the installed `.app` bundle.
fn is_bundled() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|e| crate::autostart::bundle_path(&e))
        .is_some()
}

#[cfg(target_os = "macos")]
fn prompt(kind: PermissionKind) -> Result<(), String> {
    use crate::stt::apple_speech::{SfSpeechPermission, SpeechPermissionProvider};
    if kind == PermissionKind::SpeechRecognition {
        // The OS ends a process that asks without the usage string, which only the
        // bundled app has.
        if !is_bundled() {
            return Err("speech access can only be requested from the installed app".into());
        }
        SfSpeechPermission.request(Box::new(|_| {}));
        return Ok(());
    }
    crate::audio::request_microphone_permission(Box::new(|_| {}));
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn prompt(_kind: PermissionKind) -> Result<(), String> {
    let _ = is_bundled();
    Err("permission prompts are only available on macOS".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_unknown_ids_are_rejected() {
        for kind in PermissionKind::ALL {
            assert_eq!(PermissionKind::from_id(kind.id()), Some(kind));
        }
        for bad in [
            "",
            "Accessibility",
            "camera",
            "speech",
            "microphone; rm -rf /",
            "x-apple",
        ] {
            assert_eq!(PermissionKind::from_id(bad), None, "{bad}");
        }
    }

    #[test]
    fn pane_urls_point_at_the_exact_privacy_pane() {
        let base = "x-apple.systempreferences:com.apple.preference.security?";
        assert_eq!(
            pane_url(PermissionKind::Accessibility),
            format!("{base}Privacy_Accessibility")
        );
        assert_eq!(
            pane_url(PermissionKind::InputMonitoring),
            format!("{base}Privacy_ListenEvent")
        );
        assert_eq!(
            pane_url(PermissionKind::Microphone),
            format!("{base}Privacy_Microphone")
        );
        assert_eq!(
            pane_url(PermissionKind::SpeechRecognition),
            format!("{base}Privacy_SpeechRecognition")
        );
    }

    #[test]
    fn pane_urls_have_no_spaces_and_are_distinct() {
        let urls: Vec<&str> = PermissionKind::ALL.into_iter().map(pane_url).collect();
        for url in &urls {
            assert!(!url.chars().any(char::is_whitespace), "{url}");
        }
        let mut unique = urls.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), urls.len());
    }

    #[test]
    fn only_mic_can_prompt() {
        assert!(PermissionKind::Microphone.can_prompt());
        assert!(!PermissionKind::Accessibility.can_prompt());
        assert!(!PermissionKind::InputMonitoring.can_prompt());
    }

    #[test]
    fn request_rejects_unknown_and_settings_only_kinds() {
        assert!(request_permission("camera".into()).is_err());
        assert!(request_permission("speechRecognition".into()).is_err());
        assert!(request_permission("accessibility".into()).is_err());
        assert!(request_permission("inputMonitoring".into()).is_err());
        assert!(open_permission_pane("nope".into()).is_err());
    }

    #[test]
    fn speech_can_prompt_but_never_from_an_unbundled_binary() {
        assert!(PermissionKind::SpeechRecognition.can_prompt());
        // The test binary is not inside a .app, so the OS prompt is never reached.
        assert!(request_permission("speechRecognition".into()).is_err());
    }

    #[test]
    fn tri_state_maps_to_state_and_request_flag() {
        assert_eq!(from_tri(true, false), (PermissionState::Granted, false));
        assert_eq!(from_tri(false, true), (PermissionState::Needed, true));
        assert_eq!(from_tri(false, false), (PermissionState::Needed, false));
    }

    #[test]
    fn status_serializes_as_camel_case_with_lowercase_states() {
        let json = serde_json::to_value(PermissionsStatus {
            accessibility: PermissionState::Granted,
            input_monitoring: PermissionState::Needed,
            ..PermissionsStatus::UNKNOWN
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "accessibility": "granted",
                "inputMonitoring": "needed",
                "microphone": "unknown",
                "microphoneCanRequest": false,
                "speechRecognition": "unknown",
                "speechCanRequest": false
            })
        );
    }
}
