//! Stub implementation for unsupported platforms (Linux, etc.)
//!
//! This provides a no-op implementation that logs warnings but doesn't fail.

use super::{ActiveMuteSession, AudioControlError, SystemAudioControl};
use std::sync::atomic::{AtomicBool, Ordering};

/// Stub audio controller for unsupported platforms.
///
/// All operations succeed but do nothing. Logs a warning on first use.
pub struct StubAudioController {
    warned: AtomicBool,
}

impl StubAudioController {
    pub fn new() -> Self {
        Self {
            warned: AtomicBool::new(false),
        }
    }

    fn warn_once(&self) {
        if !self.warned.swap(true, Ordering::SeqCst) {
            log::warn!(
                "Audio mute not implemented for this platform. \
                Recording will work, but system audio won't be muted."
            );
        }
    }
}

impl SystemAudioControl for StubAudioController {
    fn is_muted(&self) -> Result<bool, AudioControlError> {
        self.warn_once();
        Ok(false)
    }

    fn begin_mute_session(&self) -> Result<ActiveMuteSession, AudioControlError> {
        self.warn_once();
        Ok(ActiveMuteSession::StubNoOp)
    }

    fn end_mute_session(
        &self,
        active_mute_session: &ActiveMuteSession,
    ) -> Result<(), AudioControlError> {
        if !matches!(active_mute_session, ActiveMuteSession::StubNoOp) {
            return Err(AudioControlError::SetPropertyFailed(
                "Received non-stub mute session in stub audio controller".to_string(),
            ));
        }

        self.warn_once();
        Ok(())
    }
}
