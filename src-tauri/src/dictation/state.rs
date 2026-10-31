//! Pure dictation state machine. No I/O: inputs in, at most one effect out.

use super::Outcome;
use crate::hotkey::HotkeyAction;

/// Where one dictation currently is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictationState {
    Idle,
    Recording,
    Transcribing,
    Pasting,
    /// Visible error text; the next hotkey press starts over.
    Error(String),
}

/// Everything the machine reacts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Hotkey(HotkeyAction),
    /// Mic permission or capture start failed.
    StartFailed(String),
    Transcribed(Outcome),
    /// Paste result; the error is the status text to show.
    Pasted(Result<(), String>),
    ClearError,
}

/// Work the caller must do after a transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    StartCapture,
    StopAndTranscribe,
    DiscardCapture,
    Paste(String),
    /// An error is showing; clear it after a delay.
    ScheduleClearError,
}

#[derive(Debug)]
pub struct Machine {
    state: DictationState,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    pub fn new() -> Self {
        Self {
            state: DictationState::Idle,
        }
    }

    pub fn state(&self) -> &DictationState {
        &self.state
    }

    /// Text for the tray and window title. Empty means idle.
    pub fn status_text(&self) -> String {
        match &self.state {
            DictationState::Idle => String::new(),
            DictationState::Recording => "REC".into(),
            DictationState::Transcribing | DictationState::Pasting => "...".into(),
            DictationState::Error(msg) => msg.clone(),
        }
    }

    pub fn handle(&mut self, input: Input) -> Option<Effect> {
        use DictationState as S;
        match (&self.state, input) {
            (S::Idle | S::Error(_), Input::Hotkey(HotkeyAction::StartRecording)) => {
                self.state = S::Recording;
                Some(Effect::StartCapture)
            }
            (S::Recording, Input::Hotkey(HotkeyAction::StopRecording)) => {
                self.state = S::Transcribing;
                Some(Effect::StopAndTranscribe)
            }
            (S::Recording, Input::Hotkey(HotkeyAction::DiscardRecording)) => {
                self.state = S::Idle;
                Some(Effect::DiscardCapture)
            }
            (S::Idle | S::Recording | S::Transcribing, Input::StartFailed(msg)) => {
                self.state = S::Error(msg);
                Some(Effect::ScheduleClearError)
            }
            (S::Transcribing, Input::Transcribed(outcome)) => match outcome {
                Outcome::Text(text) => {
                    self.state = S::Pasting;
                    Some(Effect::Paste(text))
                }
                Outcome::Nothing => {
                    self.state = S::Idle;
                    None
                }
                Outcome::Failed(msg) => {
                    self.state = S::Error(msg);
                    Some(Effect::ScheduleClearError)
                }
            },
            (S::Pasting, Input::Pasted(result)) => match result {
                Ok(()) => {
                    self.state = S::Idle;
                    None
                }
                Err(msg) => {
                    self.state = S::Error(msg);
                    Some(Effect::ScheduleClearError)
                }
            },
            (S::Error(_), Input::ClearError) => {
                self.state = S::Idle;
                None
            }
            _ => None,
        }
    }
}
