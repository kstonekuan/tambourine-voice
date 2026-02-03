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
            Self::InitializationFailed(msg) => write!(f, "Audio init failed: {msg}"),
            Self::GetPropertyFailed(msg) => write!(f, "Failed to get audio property: {msg}"),
            Self::SetPropertyFailed(msg) => write!(f, "Failed to set audio property: {msg}"),
            Self::NotSupported => write!(f, "Audio control not supported on this platform"),
        }
    }
}

impl std::error::Error for AudioControlError {}

/// Trait for controlling system audio mute state.
///
/// This minimal interface allows easy migration to a cross-platform library
/// by just swapping the implementation behind `create_controller()`.
pub trait SystemAudioControl: Send + Sync {
    /// Check if system audio is muted
    fn is_muted(&self) -> Result<bool, AudioControlError>;

    /// Set system mute state
    fn set_muted(&self, muted: bool) -> Result<(), AudioControlError>;
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
        windows::WindowsAudioController::new().map(|c| Box::new(c) as Box<dyn SystemAudioControl>)
    }

    #[cfg(target_os = "macos")]
    {
        macos::MacOSAudioController::new().map(|c| Box::new(c) as Box<dyn SystemAudioControl>)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok(Box::new(stub::StubAudioController::new()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MuteState {
    #[default]
    NotMuting,
    MutedByUs,
    AudioWasAlreadyMutedByUser,
}

/// Manages muting/unmuting system audio during recording.
pub struct AudioMuteManager {
    controller: Box<dyn SystemAudioControl>,
    state: Mutex<MuteState>,
}

impl AudioMuteManager {
    pub fn new() -> Option<Self> {
        match create_controller() {
            Ok(controller) => Some(Self {
                controller,
                state: Mutex::new(MuteState::NotMuting),
            }),
            Err(e) => {
                log::warn!("Audio mute not available: {e}");
                None
            }
        }
    }

    pub fn mute(&self) -> Result<(), AudioControlError> {
        let mut state = self.state.lock().unwrap();

        if *state != MuteState::NotMuting {
            return Ok(());
        }

        let audio_is_already_muted = self.controller.is_muted().unwrap_or(false);
        if audio_is_already_muted {
            log::info!("System audio already muted, skipping");
            *state = MuteState::AudioWasAlreadyMutedByUser;
        } else {
            self.controller.set_muted(true)?;
            log::info!("System audio muted for recording");
            *state = MuteState::MutedByUs;
        }

        Ok(())
    }

    pub fn unmute(&self) -> Result<(), AudioControlError> {
        let mut state = self.state.lock().unwrap();

        match *state {
            MuteState::NotMuting => Ok(()),
            MuteState::MutedByUs => {
                self.controller.set_muted(false)?;
                log::info!("System audio unmuted after recording");
                *state = MuteState::NotMuting;
                Ok(())
            }
            MuteState::AudioWasAlreadyMutedByUser => {
                log::info!("System audio was already muted, leaving muted");
                *state = MuteState::NotMuting;
                Ok(())
            }
        }
    }
}

impl Drop for AudioMuteManager {
    fn drop(&mut self) {
        // Try to unmute on drop (app exit/crash)
        let state = self.state.lock().unwrap();
        if *state == MuteState::MutedByUs {
            drop(state); // Release lock before calling unmute
            let _ = self.unmute();
        }
    }
}
