//! Picks the recognizer for one hold. Pure: permission, on-device support and
//! the user setting in, a choice out. A fallback to Whisper always carries a
//! fixed reason string, so it is never silent.

use crate::stt::apple_speech::{LocaleSupport, SpeechPermissionStatus};

/// Preferred locale for on-device streaming recognition: Indian English.
pub const LOCALE_PREFERRED: &str = "en-IN";
/// Used when the preferred locale cannot run on this Mac.
pub const LOCALE_FALLBACK: &str = "en-US";
/// Env var that picks the locale (`DEEPAI_LOCALE=en-GB`) until a persisted
/// setting exists. Only the locales in `LOCALES` are accepted.
pub const LOCALE_ENV: &str = "DEEPAI_LOCALE";
/// Locales the app may request.
pub const LOCALES: [&str; 3] = ["en-IN", "en-US", "en-GB"];

/// Picks the locale for a hold: the env choice if it is allowed and runs on
/// device, else Indian English, else US English. Pure: `support` says what
/// the OS can do.
pub fn pick_locale(env: Option<&str>, support: impl Fn(&str) -> LocaleSupport) -> &'static str {
    let wanted = env
        .map(str::trim)
        .and_then(|v| LOCALES.iter().find(|l| l.eq_ignore_ascii_case(v)).copied());
    for candidate in wanted
        .into_iter()
        .chain([LOCALE_PREFERRED, LOCALE_FALLBACK])
    {
        if support(candidate) == LocaleSupport::OnDevice {
            return candidate;
        }
    }
    LOCALE_FALLBACK
}
/// Env var that forces Whisper (`DEEPAI_ENGINE=whisper`) until a persisted
/// setting exists.
pub const ENGINE_ENV: &str = "DEEPAI_ENGINE";

/// User setting `engine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineSetting {
    /// Streaming when it can run on this Mac, otherwise Whisper.
    #[default]
    Auto,
    /// Always Whisper.
    Whisper,
}

impl EngineSetting {
    /// Reads the env value: "whisper" (any case) forces Whisper, anything else is Auto.
    pub fn from_env_value(value: Option<&str>) -> Self {
        match value.map(|v| v.trim().to_ascii_lowercase()) {
            Some(v) if v == "whisper" => Self::Whisper,
            _ => Self::Auto,
        }
    }
}

/// Why a hold uses Whisper instead of streaming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    Setting,
    PermissionPending,
    PermissionDenied,
    NotOnDevice,
    StartFailed,
}

impl Fallback {
    /// Fixed status text. Never contains transcript text.
    pub fn status(self) -> &'static str {
        match self {
            Self::Setting => "Whisper: engine set to Whisper",
            Self::PermissionPending => "Whisper: asking for speech permission",
            Self::PermissionDenied => "Whisper: speech permission denied",
            Self::NotOnDevice => "Whisper: on-device speech unavailable",
            Self::StartFailed => "Whisper: speech engine did not start",
        }
    }
}

/// Engine for one hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineChoice {
    Streaming,
    Whisper(Fallback),
}

/// Streaming only when the setting allows it, permission is granted and the
/// locale runs on device. `support` is only called when it matters.
pub fn choose_engine(
    setting: EngineSetting,
    permission: SpeechPermissionStatus,
    support: impl FnOnce() -> LocaleSupport,
) -> EngineChoice {
    if setting == EngineSetting::Whisper {
        return EngineChoice::Whisper(Fallback::Setting);
    }
    match permission {
        SpeechPermissionStatus::Undetermined => EngineChoice::Whisper(Fallback::PermissionPending),
        SpeechPermissionStatus::Denied => EngineChoice::Whisper(Fallback::PermissionDenied),
        SpeechPermissionStatus::Granted => match support() {
            LocaleSupport::OnDevice => EngineChoice::Streaming,
            LocaleSupport::NotSupported | LocaleSupport::ServerOnly => {
                EngineChoice::Whisper(Fallback::NotOnDevice)
            }
        },
    }
}

/// Ask for speech permission at most once per run, only when it is not
/// decided, the setting allows streaming, and the app runs from its `.app`
/// bundle (the OS ends a process that asks without the usage string).
pub fn should_request_permission(
    setting: EngineSetting,
    permission: SpeechPermissionStatus,
    bundled: bool,
    already_asked: bool,
) -> bool {
    setting == EngineSetting::Auto
        && permission == SpeechPermissionStatus::Undetermined
        && bundled
        && !already_asked
}

#[cfg(test)]
mod locale_tests {
    use super::*;

    fn only(on: &'static [&'static str]) -> impl Fn(&str) -> LocaleSupport {
        move |l| {
            if on.contains(&l) {
                LocaleSupport::OnDevice
            } else {
                LocaleSupport::NotSupported
            }
        }
    }

    #[test]
    fn indian_english_is_preferred_when_it_runs_on_device() {
        assert_eq!(pick_locale(None, only(&["en-IN", "en-US"])), "en-IN");
    }

    #[test]
    fn falls_back_to_us_english_when_indian_english_is_not_on_device() {
        assert_eq!(pick_locale(None, only(&["en-US"])), "en-US");
    }

    #[test]
    fn env_choice_wins_when_allowed_and_on_device() {
        assert_eq!(
            pick_locale(Some("en-gb"), only(&["en-GB", "en-IN"])),
            "en-GB"
        );
        // Not on device: back to the normal order.
        assert_eq!(pick_locale(Some("en-GB"), only(&["en-IN"])), "en-IN");
    }

    #[test]
    fn unknown_env_values_are_ignored() {
        assert_eq!(pick_locale(Some("fr-FR"), only(&["en-IN"])), "en-IN");
        assert_eq!(pick_locale(Some("../etc"), only(&["en-US"])), "en-US");
    }

    #[test]
    fn nothing_on_device_returns_the_fallback_so_whisper_takes_over() {
        assert_eq!(pick_locale(None, only(&[])), LOCALE_FALLBACK);
    }
}
