//! Picks the recognizer for one hold. Pure: permission, on-device support and
//! the user setting in, a choice out. A fallback to Whisper always carries a
//! fixed reason string, so it is never silent.

use crate::stt::apple_speech::{LocaleSupport, SpeechPermissionStatus};

/// Locale used for on-device streaming recognition.
pub const STREAM_LOCALE: &str = "en-US";
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
