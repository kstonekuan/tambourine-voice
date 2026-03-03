//! System audio mute control for voice dictation.
//!
//! This module provides a minimal trait interface for controlling system audio,
//! making it easy to swap implementations or migrate to a cross-platform library.

use std::fmt;
use std::sync::Mutex;

// Platform-specific implementations
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod stub;
#[cfg(target_os = "windows")]
mod windows;

/// Error type for audio control operations
#[derive(Debug)]
#[allow(dead_code)] // Variants used on Windows/macOS, not Linux
pub enum AudioControlError {
    /// Platform-specific initialization failed
    InitializationFailed(String),
    /// Failed to get audio property
    GetPropertyFailed(String),
    /// Failed to set audio property
    SetPropertyFailed(String),
    /// Platform not supported
    NotSupported,
}

impl fmt::Display for AudioControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InitializationFailed(message) => write!(f, "Audio init failed: {message}"),
            Self::GetPropertyFailed(message) => {
                write!(f, "Failed to get audio property: {message}")
            }
            Self::SetPropertyFailed(message) => {
                write!(f, "Failed to set audio property: {message}")
            }
            Self::NotSupported => write!(f, "Audio control not supported on this platform"),
        }
    }
}

impl std::error::Error for AudioControlError {}

#[derive(Debug, Clone, PartialEq)]
pub struct OutputVolumeScalarSnapshot {
    pub property_element: u32,
    pub initial_volume_scalar: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveMuteSession {
    #[cfg(target_os = "windows")]
    WindowsEndpointMute,
    #[cfg(target_os = "macos")]
    MacOsDeviceMute {
        output_device_id: u32,
        initial_device_muted: bool,
    },
    #[cfg(target_os = "macos")]
    MacOsVolumeZeroFallback {
        output_device_id: u32,
        captured_volume_scalars: Vec<OutputVolumeScalarSnapshot>,
    },
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    StubNoOp,
}

/// Trait for controlling system audio mute state.
///
/// This minimal interface allows easy migration to a cross-platform library
/// by just swapping the implementation behind `create_controller()`.
pub trait SystemAudioControl: Send + Sync {
    /// Check if system audio is muted
    fn is_muted(&self) -> Result<bool, AudioControlError>;

    /// Start a new mute session and return the session token needed for restoration.
    fn begin_mute_session(&self) -> Result<ActiveMuteSession, AudioControlError>;

    /// Restore audio state from an active mute session.
    fn end_mute_session(
        &self,
        active_mute_session: &ActiveMuteSession,
    ) -> Result<(), AudioControlError>;
}

/// Check if audio mute is supported on this platform.
pub fn is_supported() -> bool {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        true
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        false
    }
}

/// Create a platform-appropriate audio controller.
///
/// Returns a boxed trait object that can control system audio.
/// On unsupported platforms, returns a stub that does nothing.
pub fn create_controller() -> Result<Box<dyn SystemAudioControl>, AudioControlError> {
    #[cfg(target_os = "windows")]
    {
        windows::WindowsAudioController::new()
            .map(|controller| Box::new(controller) as Box<dyn SystemAudioControl>)
    }

    #[cfg(target_os = "macos")]
    {
        macos::MacOSAudioController::new()
            .map(|controller| Box::new(controller) as Box<dyn SystemAudioControl>)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok(Box::new(stub::StubAudioController::new()))
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MuteState {
    #[default]
    NotMuting,
    MutedByUs,
    AudioWasAlreadyMutedByUser,
}

#[derive(Debug, Clone, PartialEq, Default)]
enum AudioMuteManagerState {
    #[default]
    NotMuting,
    MutedByUs {
        active_mute_session: ActiveMuteSession,
    },
    AudioWasAlreadyMutedByUser,
}

impl AudioMuteManagerState {
    #[cfg(test)]
    fn as_public_mute_state(&self) -> MuteState {
        match self {
            Self::NotMuting => MuteState::NotMuting,
            Self::MutedByUs { .. } => MuteState::MutedByUs,
            Self::AudioWasAlreadyMutedByUser => MuteState::AudioWasAlreadyMutedByUser,
        }
    }
}

/// Manages muting/unmuting system audio during recording.
pub struct AudioMuteManager {
    controller: Box<dyn SystemAudioControl>,
    state: Mutex<AudioMuteManagerState>,
}

impl AudioMuteManager {
    pub fn new() -> Option<Self> {
        match create_controller() {
            Ok(controller) => Some(Self::from_controller(controller)),
            Err(error) => {
                log::warn!("Audio mute not available: {error}");
                None
            }
        }
    }

    pub fn from_controller(controller: Box<dyn SystemAudioControl>) -> Self {
        Self {
            controller,
            state: Mutex::new(AudioMuteManagerState::NotMuting),
        }
    }

    #[cfg(test)]
    pub(crate) fn current_state(&self) -> MuteState {
        self.state.lock().unwrap().as_public_mute_state()
    }

    pub fn mute(&self) -> Result<(), AudioControlError> {
        let mut state_guard = self.state.lock().unwrap();

        if !matches!(*state_guard, AudioMuteManagerState::NotMuting) {
            return Ok(());
        }

        let audio_is_already_muted = self.controller.is_muted().unwrap_or(false);
        if audio_is_already_muted {
            *state_guard = AudioMuteManagerState::AudioWasAlreadyMutedByUser;
            log::info!("System audio already muted, skipping");
            return Ok(());
        }

        let active_mute_session = self.controller.begin_mute_session()?;
        *state_guard = AudioMuteManagerState::MutedByUs {
            active_mute_session,
        };
        log::info!("System audio muted for recording");
        Ok(())
    }

    pub fn unmute(&self) -> Result<(), AudioControlError> {
        let mut state_guard = self.state.lock().unwrap();

        match &*state_guard {
            AudioMuteManagerState::MutedByUs {
                active_mute_session,
            } => {
                self.controller.end_mute_session(active_mute_session)?;
                *state_guard = AudioMuteManagerState::NotMuting;
                log::info!("System audio unmuted after recording");
            }
            AudioMuteManagerState::AudioWasAlreadyMutedByUser => {
                *state_guard = AudioMuteManagerState::NotMuting;
                log::info!("System audio was already muted, leaving muted");
            }
            AudioMuteManagerState::NotMuting => {}
        }

        Ok(())
    }
}

impl Drop for AudioMuteManager {
    fn drop(&mut self) {
        // Try to unmute on drop (app exit/crash)
        let state_guard = self.state.lock().unwrap();
        if matches!(*state_guard, AudioMuteManagerState::MutedByUs { .. }) {
            drop(state_guard); // Release lock before calling unmute
            let _ = self.unmute();
        }
    }
}

#[cfg(test)]
#[path = "../tests/audio_mute_tests.rs"]
mod audio_mute_tests;
