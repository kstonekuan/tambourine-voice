//! System audio mute control for voice dictation.
//!
//! This module provides a minimal trait interface for controlling system audio,
//! making it easy to swap implementations or migrate to a cross-platform library.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

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

/// Manages muting/unmuting system audio during recording.
///
/// Tracks whether audio was muted before we started, so we can restore
/// the correct state after recording ends.
pub struct AudioMuteManager {
    controller: Box<dyn SystemAudioControl>,
    /// Was audio already muted before we started muting?
    was_muted_before: AtomicBool,
    /// Are we currently in a muted state (that we caused)?
    is_currently_muting: AtomicBool,
}

impl AudioMuteManager {
    /// Create a new `AudioMuteManager`.
    ///
    /// Returns None if audio control is not available on this platform.
    pub fn new() -> Option<Self> {
        match create_controller() {
            Ok(controller) => Some(Self {
                controller,
                was_muted_before: AtomicBool::new(false),
                is_currently_muting: AtomicBool::new(false),
            }),
            Err(e) => {
                log::warn!("Audio mute not available: {e}");
                None
            }
        }
    }

    /// Mute system audio for recording.
    ///
    /// Saves the current mute state so it can be restored later.
    /// If already muting, this is a no-op.
    pub fn mute(&self) -> Result<(), AudioControlError> {
        // Check if we're already muting
        if self.is_currently_muting.swap(true, Ordering::SeqCst) {
            return Ok(()); // Already muting, nothing to do
        }

        // Check current mute state and save it
        let was_muted = self.controller.is_muted().unwrap_or(false);
        self.was_muted_before.store(was_muted, Ordering::SeqCst);

        // Only mute if not already muted
        if was_muted {
            log::info!("System audio already muted, skipping");
        } else {
            self.controller.set_muted(true)?;
            log::info!("System audio muted for recording");
        }

        Ok(())
    }

    /// Unmute system audio after recording.
    ///
    /// Only unmutes if we were the ones who muted it.
    /// If not currently muting, this is a no-op.
    pub fn unmute(&self) -> Result<(), AudioControlError> {
        // Check if we're currently muting
        if !self.is_currently_muting.swap(false, Ordering::SeqCst) {
            return Ok(()); // Not muting, nothing to do
        }

        // Only unmute if it wasn't already muted before we started
        if self.was_muted_before.load(Ordering::SeqCst) {
            log::info!("System audio was already muted, leaving muted");
        } else {
            self.controller.set_muted(false)?;
            log::info!("System audio unmuted after recording");
        }

        Ok(())
    }
}

impl Drop for AudioMuteManager {
    fn drop(&mut self) {
        // Try to unmute on drop (app exit/crash)
        if self.is_currently_muting.load(Ordering::SeqCst) {
            let _ = self.unmute();
        }
    }
}
