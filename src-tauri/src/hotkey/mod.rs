//! Hotkey module: hold-to-talk key capture with Fn (Globe) primary and
//! Ctrl+Option+Space fallback.
//!
//! Architecture
//! ------------
//! - `HotkeyEvent`  -- the two logical events emitted by any key source.
//! - `HotkeyConfig` -- runtime-tunable parameters (thresholds, shortcut string).
//! - `HotkeyState`  -- state machine: Idle -> Recording -> Idle, with a
//!   side-track Busy state during transcription/paste.
//! - `HotkeyController` -- turns timed events into start/stop/discard actions.
//! - `fallback`     -- Ctrl+Option+Space via tauri-plugin-global-shortcut.
//! - `filter`       -- pure functions for quick-tap discard and busy guard.
//! - `macos`        -- macOS Fn key source (CGEventTap) and a test mock.
//!
//! Unit tests live in `src-tauri/tests/hotkey_test.rs` and require no
//! runtime permissions.

pub mod fallback;
pub mod filter;

#[cfg(target_os = "macos")]
pub mod macos;

// ---------------------------------------------------------------------------
// HotkeyEvent
// ---------------------------------------------------------------------------

/// Logical events produced by any key-capture backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// The hotkey was pressed (key-down or flags-changed to Fn-on).
    Press,
    /// The hotkey was released (key-up or flags-changed to Fn-off).
    Release,
}

// ---------------------------------------------------------------------------
// HotkeyConfig
// ---------------------------------------------------------------------------

/// Runtime configuration for the hotkey subsystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyConfig {
    /// When `true`, the primary key is Fn (Globe) via CGEventTap.
    /// When `false`, only the fallback shortcut is active.
    pub primary_is_fn: bool,

    /// Cross-platform fallback shortcut registered via tauri-plugin-global-shortcut.
    pub fallback_shortcut: String,

    /// Minimum hold duration in milliseconds before a press/release pair is
    /// treated as intentional. Taps shorter than this are silently discarded.
    pub quick_tap_threshold_ms: u64,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            primary_is_fn: true,
            fallback_shortcut: "Ctrl+Option+Space".to_string(),
            quick_tap_threshold_ms: 300,
        }
    }
}

// ---------------------------------------------------------------------------
// HotkeyState
// ---------------------------------------------------------------------------

/// State machine for hold-to-talk capture.
///
/// ```text
/// Idle  --[Press]--> Recording  --[Release]--> Idle
///                                 |
///              [transcribe/paste starts] --> Busy
///              [transcribe/paste ends]   --> Idle
/// ```
///
/// Press events received while in `Recording` or `Busy` are ignored (see
/// `filter::apply_busy_guard`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyState {
    /// No hotkey held; ready to start recording.
    Idle,
    /// Hotkey is held down; audio capture is active.
    Recording,
    /// Transcription or paste is in progress; further hotkey events are ignored.
    Busy,
}

impl HotkeyState {
    /// Drive the state machine forward given an incoming `event`.
    ///
    /// Transitions:
    /// - `Idle + Press    => Recording`
    /// - `Recording + Release => Idle`
    /// - `Recording + Press   => Recording` (no double-entry)
    /// - `Busy + Press    => Busy`   (ignored)
    /// - `Busy + Release  => Idle`
    /// - `Idle + Release  => Idle`   (harmless no-op)
    pub fn transition(&mut self, event: HotkeyEvent) {
        *self = match (*self, event) {
            (HotkeyState::Idle, HotkeyEvent::Press) => HotkeyState::Recording,
            (HotkeyState::Recording, HotkeyEvent::Release) => HotkeyState::Idle,
            (HotkeyState::Recording, HotkeyEvent::Press) => HotkeyState::Recording,
            (HotkeyState::Busy, HotkeyEvent::Release) => HotkeyState::Idle,
            (HotkeyState::Busy, HotkeyEvent::Press) => HotkeyState::Busy,
            (HotkeyState::Idle, HotkeyEvent::Release) => HotkeyState::Idle,
        };
    }

    /// Enter the Busy state (called when transcription/paste begins).
    pub fn mark_busy(&mut self) {
        *self = HotkeyState::Busy;
    }

    /// Leave the Busy state (called when transcription/paste finishes).
    pub fn mark_idle(&mut self) {
        *self = HotkeyState::Idle;
    }
}

// ---------------------------------------------------------------------------
// HotkeyController
// ---------------------------------------------------------------------------

/// What the recorder should do after a hotkey event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    /// Start capturing audio.
    StartRecording,
    /// Held long enough: stop capturing and transcribe.
    StopRecording,
    /// Released before the quick-tap threshold: stop capturing and drop it.
    DiscardRecording,
    /// Event was ignored (busy guard, unpaired release).
    None,
}

/// Combines the state machine, busy guard and quick-tap filter.
///
/// Timestamps are passed in so tests are deterministic. Only press/release
/// events and timings are ever seen here, never key contents.
#[derive(Debug)]
pub struct HotkeyController {
    state: HotkeyState,
    threshold: std::time::Duration,
    pressed_at: Option<std::time::Instant>,
    /// True while a press was ignored; its matching release is ignored too.
    ignoring_release: bool,
}

impl HotkeyController {
    pub fn new(config: &HotkeyConfig) -> Self {
        Self {
            state: HotkeyState::Idle,
            threshold: std::time::Duration::from_millis(config.quick_tap_threshold_ms),
            pressed_at: None,
            ignoring_release: false,
        }
    }

    pub fn state(&self) -> HotkeyState {
        self.state
    }

    /// Call when transcription or paste begins.
    pub fn mark_busy(&mut self) {
        self.state.mark_busy();
    }

    /// Call when transcription and paste are finished.
    pub fn mark_idle(&mut self) {
        self.state.mark_idle();
    }

    /// Feed one event observed at `now`.
    pub fn handle(&mut self, event: HotkeyEvent, now: std::time::Instant) -> HotkeyAction {
        match event {
            HotkeyEvent::Press => {
                if !filter::apply_busy_guard(&self.state, event) {
                    self.ignoring_release = true;
                    return HotkeyAction::None;
                }
                self.ignoring_release = false;
                self.state.transition(event);
                self.pressed_at = Some(now);
                HotkeyAction::StartRecording
            }
            HotkeyEvent::Release => {
                if self.ignoring_release {
                    self.ignoring_release = false;
                    return HotkeyAction::None;
                }
                let Some(start) = self.pressed_at.take() else {
                    return HotkeyAction::None;
                };
                self.state.transition(event);
                if now.saturating_duration_since(start) >= self.threshold {
                    HotkeyAction::StopRecording
                } else {
                    HotkeyAction::DiscardRecording
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Permission
// ---------------------------------------------------------------------------

/// System Settings pane for Input Monitoring.
pub const INPUT_MONITORING_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent";

/// Outcome of the Input Monitoring permission check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionCheck {
    Granted,
    /// Missing: the caller should open `deep_link` and use the fallback key.
    Missing {
        deep_link: &'static str,
    },
}

/// Maps a granted flag to the check result (pure, testable).
pub fn permission_check(granted: bool) -> PermissionCheck {
    if granted {
        PermissionCheck::Granted
    } else {
        PermissionCheck::Missing {
            deep_link: INPUT_MONITORING_SETTINGS_URL,
        }
    }
}

/// Result of trying to start the Fn key tap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FnTapStatus {
    Started,
    /// Input Monitoring is not granted. Use the fallback shortcut; call
    /// `retry_fn_tap` after the user grants it.
    PermissionMissing,
    Failed(String),
}

#[cfg(target_os = "macos")]
static FN_TAP_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Preflights permission and starts the Fn tap. With `prompt` set and the
/// permission missing, shows the system prompt once. Never opens Settings.
#[cfg(target_os = "macos")]
pub fn start_fn_tap(tx: std::sync::mpsc::Sender<HotkeyEvent>, prompt: bool) -> FnTapStatus {
    use std::sync::atomic::Ordering;
    if FN_TAP_STARTED.load(Ordering::SeqCst) {
        return FnTapStatus::Started;
    }
    if let PermissionCheck::Missing { .. } = permission_check(macos::has_permission()) {
        if prompt {
            macos::request_permission();
        }
        return FnTapStatus::PermissionMissing;
    }
    match macos::install_cg_event_tap() {
        Ok(fn_rx) => {
            FN_TAP_STARTED.store(true, Ordering::SeqCst);
            std::thread::spawn(move || {
                for ev in fn_rx {
                    let _ = tx.send(ev);
                }
            });
            FnTapStatus::Started
        }
        Err(e) => FnTapStatus::Failed(e),
    }
}

/// Starts the Fn tap later (for example after the user grants permission).
/// No prompt, no polling; call it from an explicit user action.
#[cfg(target_os = "macos")]
pub fn retry_fn_tap(tx: std::sync::mpsc::Sender<HotkeyEvent>) -> FnTapStatus {
    start_fn_tap(tx, false)
}

/// Explicit action: opens the Input Monitoring pane in System Settings.
pub fn open_input_monitoring_settings() {
    let _ = std::process::Command::new("open")
        .arg(INPUT_MONITORING_SETTINGS_URL)
        .spawn();
}

// ---------------------------------------------------------------------------
// Startup wiring
// ---------------------------------------------------------------------------

/// Starts both key sources and the controller loop.
///
/// `tx` must be the sender given to `fallback::plugin`. Every action other
/// than `None` is passed to `on_action` (called on the controller thread, so
/// it must not block). Event kind and timing only are logged.
pub fn start<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    tx: std::sync::mpsc::Sender<HotkeyEvent>,
    rx: std::sync::mpsc::Receiver<HotkeyEvent>,
    on_action: impl Fn(HotkeyAction) + Send + 'static,
) {
    let config = HotkeyConfig::default();
    // Fn is the main key. The backup shortcut is registered only when Fn
    // cannot be used (no permission, tap failed, or not macOS).
    let register_fallback = |app: &tauri::AppHandle<R>| {
        if let Err(e) = fallback::register(app, &config.fallback_shortcut) {
            tracing::warn!("fallback shortcut not registered: {e}");
        }
    };

    #[cfg(not(target_os = "macos"))]
    register_fallback(app);

    #[cfg(target_os = "macos")]
    if !config.primary_is_fn {
        register_fallback(app);
    } else {
        let retry_tx = tx.clone();
        let retry_app = app.clone();
        let retry_shortcut = config.fallback_shortcut.clone();
        // No automatic system prompt at launch: the user grants Input Monitoring from
        // Settings > Permissions, so no popup flashes at every start.
        match start_fn_tap(tx, false) {
            FnTapStatus::Started => {}
            FnTapStatus::PermissionMissing => {
                register_fallback(app);
                tracing::warn!("Input Monitoring missing, using fallback shortcut");
                // Re-check every 3 s so granting the permission works without a
                // restart. Stops once the tap starts or fails for another reason.
                std::thread::spawn(move || {
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        match retry_fn_tap(retry_tx.clone()) {
                            FnTapStatus::PermissionMissing => continue,
                            FnTapStatus::Started => {
                                tracing::info!("Input Monitoring granted, Fn key tap started");
                                let _ = fallback::unregister(&retry_app, &retry_shortcut);
                                break;
                            }
                            FnTapStatus::Failed(e) => {
                                tracing::warn!("Fn key tap unavailable after retry: {e}");
                                break;
                            }
                        }
                    }
                });
            }
            FnTapStatus::Failed(e) => {
                register_fallback(app);
                tracing::warn!("Fn key tap unavailable, using fallback: {e}");
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    drop(tx);

    std::thread::spawn(move || {
        let mut controller = HotkeyController::new(&config);
        for ev in rx {
            let action = controller.handle(ev, std::time::Instant::now());
            tracing::debug!(?ev, ?action, "hotkey");
            if action != HotkeyAction::None {
                on_action(action);
            }
        }
    });
}
