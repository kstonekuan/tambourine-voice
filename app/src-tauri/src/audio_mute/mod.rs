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

#[cfg(target_os = "macos")]
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

impl ActiveMuteSession {
    fn telemetry_summary(&self) -> String {
        match self {
            #[cfg(target_os = "windows")]
            Self::WindowsEndpointMute => "windows_endpoint_mute".to_string(),
            #[cfg(target_os = "macos")]
            Self::MacOsDeviceMute {
                output_device_id, ..
            } => format!("macos_device_mute(output_device_id={output_device_id})"),
            #[cfg(target_os = "macos")]
            Self::MacOsVolumeZeroFallback {
                output_device_id,
                captured_volume_scalars,
            } => format!(
                "macos_volume_zero_fallback(output_device_id={output_device_id},captured_scalar_count={})",
                captured_volume_scalars.len()
            ),
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            Self::StubNoOp => "stub_no_op".to_string(),
        }
    }
}

/// Error type for audio control operations
#[derive(Debug, Clone)]
#[allow(dead_code)] // Variants used on Windows/macOS, not Linux
pub enum AudioControlError {
    /// Platform-specific initialization failed
    InitializationFailed(String),
    /// Failed to get audio property
    GetPropertyFailed(String),
    /// Failed to set audio property
    SetPropertyFailed(String),
    /// Failed to start mute session. May contain a recovery session that can be used to restore state.
    MuteSessionStartFailed {
        message: String,
        recovery_session: Option<ActiveMuteSession>,
    },
    /// Platform not supported
    NotSupported,
}

impl AudioControlError {
    fn recovery_session(&self) -> Option<&ActiveMuteSession> {
        match self {
            Self::MuteSessionStartFailed {
                recovery_session: Some(active_mute_session),
                ..
            } => Some(active_mute_session),
            _ => None,
        }
    }
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
            Self::MuteSessionStartFailed { message, .. } => {
                write!(f, "Failed to start mute session: {message}")
            }
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
    RecoveryPending,
    AudioWasAlreadyMutedByUser,
}

#[derive(Debug, Clone, PartialEq, Default)]
enum AudioMuteManagerState {
    #[default]
    NotMuting,
    MutedByUs {
        active_mute_session: ActiveMuteSession,
    },
    RecoveryPending {
        active_mute_session: ActiveMuteSession,
        mute_error_message: String,
    },
    AudioWasAlreadyMutedByUser,
}

impl AudioMuteManagerState {
    #[cfg(test)]
    fn as_public_mute_state(&self) -> MuteState {
        match self {
            Self::NotMuting => MuteState::NotMuting,
            Self::MutedByUs { .. } => MuteState::MutedByUs,
            Self::RecoveryPending { .. } => MuteState::RecoveryPending,
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

        match &*state_guard {
            AudioMuteManagerState::NotMuting => {}
            AudioMuteManagerState::MutedByUs { .. }
            | AudioMuteManagerState::AudioWasAlreadyMutedByUser => return Ok(()),
            AudioMuteManagerState::RecoveryPending {
                active_mute_session,
                mute_error_message,
            } => {
                let blocked_mute_error_message = format!(
                    "Cannot start a new mute operation while recovery is pending from a previous mute failure: {mute_error_message}"
                );
                log::warn!(
                    "Audio mute transition=BlockedMute reason=recovery_pending recovery_session={} previous_mute_error=\"{}\"",
                    active_mute_session.telemetry_summary(),
                    mute_error_message,
                );
                return Err(AudioControlError::SetPropertyFailed(
                    blocked_mute_error_message,
                ));
            }
        }

        let audio_is_already_muted = self.controller.is_muted().unwrap_or(false);
        if audio_is_already_muted {
            *state_guard = AudioMuteManagerState::AudioWasAlreadyMutedByUser;
            log::info!("System audio already muted, skipping");
            return Ok(());
        }

        match self.controller.begin_mute_session() {
            Ok(active_mute_session) => {
                let active_mute_session_telemetry = active_mute_session.telemetry_summary();
                *state_guard = AudioMuteManagerState::MutedByUs {
                    active_mute_session,
                };
                log::info!(
                    "System audio muted for recording using session {active_mute_session_telemetry}"
                );
                Ok(())
            }
            Err(begin_mute_error) => {
                if let Some(recovery_session) = begin_mute_error.recovery_session() {
                    let recovery_session_telemetry = recovery_session.telemetry_summary();
                    *state_guard = AudioMuteManagerState::RecoveryPending {
                        active_mute_session: recovery_session.clone(),
                        mute_error_message: begin_mute_error.to_string(),
                    };
                    log::warn!(
                        "Audio mute transition=RecoveryPending reason=begin_mute_failed recovery_session={recovery_session_telemetry} error={begin_mute_error}",
                    );
                }
                Err(begin_mute_error)
            }
        }
    }

    pub fn unmute(&self) -> Result<(), AudioControlError> {
        let mut state_guard = self.state.lock().unwrap();

        match &*state_guard {
            AudioMuteManagerState::MutedByUs {
                active_mute_session,
            } => {
                let active_mute_session_telemetry = active_mute_session.telemetry_summary();
                if let Err(end_mute_error) = self.controller.end_mute_session(active_mute_session) {
                    log::warn!(
                        "Audio mute transition=NotMuting failed_from=MutedByUs session={active_mute_session_telemetry} error={end_mute_error}"
                    );
                    return Err(end_mute_error);
                }
                *state_guard = AudioMuteManagerState::NotMuting;
                log::info!(
                    "System audio unmuted after recording using session {active_mute_session_telemetry}"
                );
            }
            AudioMuteManagerState::RecoveryPending {
                active_mute_session,
                ..
            } => {
                let recovery_session_telemetry = active_mute_session.telemetry_summary();
                log::info!(
                    "Audio mute transition=RecoveryAttempt recovery_session={recovery_session_telemetry}"
                );
                if let Err(recovery_error) = self.controller.end_mute_session(active_mute_session) {
                    log::warn!(
                        "Audio mute transition=RecoveryPending failed_from=RecoveryAttempt recovery_session={recovery_session_telemetry} error={recovery_error}"
                    );
                    return Err(recovery_error);
                }
                *state_guard = AudioMuteManagerState::NotMuting;
                log::info!(
                    "Audio mute transition=NotMuting completed_from=RecoveryPending recovery_session={recovery_session_telemetry}"
                );
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
        if matches!(
            *state_guard,
            AudioMuteManagerState::MutedByUs { .. } | AudioMuteManagerState::RecoveryPending { .. }
        ) {
            drop(state_guard); // Release lock before calling unmute
            if let Err(drop_cleanup_error) = self.unmute() {
                log::warn!("Audio mute transition=DropCleanupFailed error={drop_cleanup_error}");
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/audio_mute_tests.rs"]
mod audio_mute_tests;
