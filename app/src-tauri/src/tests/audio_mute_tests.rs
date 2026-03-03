use std::sync::{Arc, Mutex};

use super::{
    ActiveMuteSession, AudioControlError, AudioMuteManager, MuteState, SystemAudioControl,
};

#[derive(Debug, Default)]
struct FakeAudioControllerState {
    is_muted: bool,
    is_muted_error: Option<String>,
    begin_mute_error: Option<String>,
    begin_mute_error_with_recovery_session: Option<String>,
    end_mute_error: Option<String>,
    begin_mute_call_count: usize,
    end_mute_call_count: usize,
}

#[derive(Clone)]
struct FakeAudioController {
    state: Arc<Mutex<FakeAudioControllerState>>,
}

impl FakeAudioController {
    fn new(state: Arc<Mutex<FakeAudioControllerState>>) -> Self {
        Self { state }
    }
}

fn fake_active_mute_session() -> ActiveMuteSession {
    #[cfg(target_os = "macos")]
    {
        ActiveMuteSession::MacOsDeviceMute {
            output_device_id: 42,
            initial_device_muted: false,
        }
    }

    #[cfg(target_os = "windows")]
    {
        ActiveMuteSession::WindowsEndpointMute
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        ActiveMuteSession::StubNoOp
    }
}

fn is_expected_fake_session(active_mute_session: &ActiveMuteSession) -> bool {
    #[cfg(target_os = "macos")]
    {
        matches!(
            active_mute_session,
            ActiveMuteSession::MacOsDeviceMute { .. }
        )
    }

    #[cfg(target_os = "windows")]
    {
        matches!(active_mute_session, ActiveMuteSession::WindowsEndpointMute)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        matches!(active_mute_session, ActiveMuteSession::StubNoOp)
    }
}

impl SystemAudioControl for FakeAudioController {
    fn is_muted(&self) -> Result<bool, AudioControlError> {
        let state = self.state.lock().unwrap();
        if let Some(error_message) = &state.is_muted_error {
            return Err(AudioControlError::GetPropertyFailed(error_message.clone()));
        }

        Ok(state.is_muted)
    }

    fn begin_mute_session(&self) -> Result<ActiveMuteSession, AudioControlError> {
        let mut state = self.state.lock().unwrap();
        state.begin_mute_call_count += 1;

        if let Some(error_message) = &state.begin_mute_error {
            return Err(AudioControlError::SetPropertyFailed(error_message.clone()));
        }

        if let Some(error_message) = state.begin_mute_error_with_recovery_session.clone() {
            state.is_muted = true;
            return Err(AudioControlError::MuteSessionStartFailed {
                message: error_message,
                recovery_session: Some(fake_active_mute_session()),
            });
        }

        state.is_muted = true;
        Ok(fake_active_mute_session())
    }

    fn end_mute_session(
        &self,
        active_mute_session: &ActiveMuteSession,
    ) -> Result<(), AudioControlError> {
        let mut state = self.state.lock().unwrap();
        state.end_mute_call_count += 1;

        if !is_expected_fake_session(active_mute_session) {
            return Err(AudioControlError::SetPropertyFailed(
                "unexpected fake mute session type".to_string(),
            ));
        }

        if let Some(error_message) = &state.end_mute_error {
            return Err(AudioControlError::SetPropertyFailed(error_message.clone()));
        }

        state.is_muted = false;
        Ok(())
    }
}

#[test]
fn mute_and_unmute_perform_expected_session_calls() {
    let fake_controller_state = Arc::new(Mutex::new(FakeAudioControllerState::default()));
    let fake_controller = FakeAudioController::new(fake_controller_state.clone());
    let audio_mute_manager = AudioMuteManager::from_controller(Box::new(fake_controller));

    audio_mute_manager.mute().unwrap();
    audio_mute_manager.unmute().unwrap();

    let state_after_operations = fake_controller_state.lock().unwrap();
    assert_eq!(state_after_operations.begin_mute_call_count, 1);
    assert_eq!(state_after_operations.end_mute_call_count, 1);
    assert!(!state_after_operations.is_muted);
}

#[test]
fn mute_is_idempotent_when_already_muted_by_manager() {
    let fake_controller_state = Arc::new(Mutex::new(FakeAudioControllerState::default()));
    let fake_controller = FakeAudioController::new(fake_controller_state.clone());
    let audio_mute_manager = AudioMuteManager::from_controller(Box::new(fake_controller));

    audio_mute_manager.mute().unwrap();
    audio_mute_manager.mute().unwrap();

    let state_after_operations = fake_controller_state.lock().unwrap();
    assert_eq!(state_after_operations.begin_mute_call_count, 1);
}

#[test]
fn mute_and_unmute_preserve_user_muted_audio() {
    let fake_controller_state = Arc::new(Mutex::new(FakeAudioControllerState {
        is_muted: true,
        ..Default::default()
    }));
    let fake_controller = FakeAudioController::new(fake_controller_state.clone());
    let audio_mute_manager = AudioMuteManager::from_controller(Box::new(fake_controller));

    audio_mute_manager.mute().unwrap();
    audio_mute_manager.unmute().unwrap();

    let state_after_operations = fake_controller_state.lock().unwrap();
    assert_eq!(state_after_operations.begin_mute_call_count, 0);
    assert_eq!(state_after_operations.end_mute_call_count, 0);
    assert!(state_after_operations.is_muted);
}

#[test]
fn mute_falls_back_to_not_muted_when_is_muted_query_fails() {
    let fake_controller_state = Arc::new(Mutex::new(FakeAudioControllerState {
        is_muted_error: Some("query failure".to_string()),
        ..Default::default()
    }));
    let fake_controller = FakeAudioController::new(fake_controller_state.clone());
    let audio_mute_manager = AudioMuteManager::from_controller(Box::new(fake_controller));

    audio_mute_manager.mute().unwrap();

    let state_after_operations = fake_controller_state.lock().unwrap();
    assert_eq!(state_after_operations.begin_mute_call_count, 1);
}

#[test]
fn unmute_failure_keeps_muted_state_for_retry() {
    let fake_controller_state = Arc::new(Mutex::new(FakeAudioControllerState {
        end_mute_error: Some("restore failure".to_string()),
        ..Default::default()
    }));
    let fake_controller = FakeAudioController::new(fake_controller_state.clone());
    let audio_mute_manager = AudioMuteManager::from_controller(Box::new(fake_controller));

    audio_mute_manager.mute().unwrap();
    assert!(audio_mute_manager.unmute().is_err());
    assert_eq!(audio_mute_manager.current_state(), MuteState::MutedByUs);

    {
        let mut state_for_retry = fake_controller_state.lock().unwrap();
        state_for_retry.end_mute_error = None;
    }

    audio_mute_manager.unmute().unwrap();
    assert_eq!(audio_mute_manager.current_state(), MuteState::NotMuting);

    let state_after_retry = fake_controller_state.lock().unwrap();
    assert_eq!(state_after_retry.end_mute_call_count, 2);
    assert!(!state_after_retry.is_muted);
}

#[test]
fn mute_failure_with_recovery_session_keeps_recovery_state_until_unmute() {
    let fake_controller_state = Arc::new(Mutex::new(FakeAudioControllerState {
        begin_mute_error_with_recovery_session: Some(
            "partial mute and rollback failed".to_string(),
        ),
        ..Default::default()
    }));
    let fake_controller = FakeAudioController::new(fake_controller_state.clone());
    let audio_mute_manager = AudioMuteManager::from_controller(Box::new(fake_controller));

    assert!(audio_mute_manager.mute().is_err());
    assert_eq!(
        audio_mute_manager.current_state(),
        MuteState::RecoveryPending
    );

    let state_after_failed_mute = fake_controller_state.lock().unwrap();
    assert_eq!(state_after_failed_mute.begin_mute_call_count, 1);
    assert_eq!(state_after_failed_mute.end_mute_call_count, 0);
    assert!(state_after_failed_mute.is_muted);
    drop(state_after_failed_mute);

    audio_mute_manager.unmute().unwrap();
    assert_eq!(audio_mute_manager.current_state(), MuteState::NotMuting);

    let state_after_recovery = fake_controller_state.lock().unwrap();
    assert_eq!(state_after_recovery.end_mute_call_count, 1);
    assert!(!state_after_recovery.is_muted);
}

#[test]
fn mute_returns_error_while_recovery_is_pending() {
    let fake_controller_state = Arc::new(Mutex::new(FakeAudioControllerState {
        begin_mute_error_with_recovery_session: Some(
            "partial mute and rollback failed".to_string(),
        ),
        ..Default::default()
    }));
    let fake_controller = FakeAudioController::new(fake_controller_state);
    let audio_mute_manager = AudioMuteManager::from_controller(Box::new(fake_controller));

    assert!(audio_mute_manager.mute().is_err());
    let second_mute_result = audio_mute_manager.mute();
    assert!(matches!(
        second_mute_result,
        Err(AudioControlError::SetPropertyFailed(error_message))
        if error_message.contains("recovery is pending")
    ));
}

#[test]
fn drop_unmutes_when_manager_muted_audio() {
    let fake_controller_state = Arc::new(Mutex::new(FakeAudioControllerState::default()));
    let fake_controller = FakeAudioController::new(fake_controller_state.clone());

    {
        let audio_mute_manager = AudioMuteManager::from_controller(Box::new(fake_controller));
        audio_mute_manager.mute().unwrap();
    }

    let state_after_drop = fake_controller_state.lock().unwrap();
    assert_eq!(state_after_drop.begin_mute_call_count, 1);
    assert_eq!(state_after_drop.end_mute_call_count, 1);
    assert!(!state_after_drop.is_muted);
}
