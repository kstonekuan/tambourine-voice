use std::sync::{Arc, Mutex};

use super::shared::{
    decide_mute_transition, decide_unmute_transition, MuteTransitionAction, MuteTransitionDecision,
};
use super::{AudioControlError, AudioMuteManager, MuteState, SystemAudioControl};

#[derive(Debug, Default)]
struct FakeAudioControllerState {
    is_muted: bool,
    is_muted_error: Option<String>,
    set_muted_error: Option<String>,
    set_muted_calls: Vec<bool>,
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

impl SystemAudioControl for FakeAudioController {
    fn is_muted(&self) -> Result<bool, AudioControlError> {
        let state = self.state.lock().unwrap();
        if let Some(error_message) = &state.is_muted_error {
            return Err(AudioControlError::GetPropertyFailed(error_message.clone()));
        }

        Ok(state.is_muted)
    }

    fn set_muted(&self, muted: bool) -> Result<(), AudioControlError> {
        let mut state = self.state.lock().unwrap();
        state.set_muted_calls.push(muted);
        if let Some(error_message) = &state.set_muted_error {
            return Err(AudioControlError::SetPropertyFailed(error_message.clone()));
        }

        state.is_muted = muted;
        Ok(())
    }
}

#[test]
fn decide_mute_transition_for_not_muting_and_already_muted_returns_no_op() {
    let transition_decision = decide_mute_transition(MuteState::NotMuting, true);
    assert_eq!(
        transition_decision,
        MuteTransitionDecision {
            next_state: MuteState::AudioWasAlreadyMutedByUser,
            action: MuteTransitionAction::NoOp,
        }
    );
}

#[test]
fn decide_mute_transition_for_not_muting_and_not_muted_sets_muted() {
    let transition_decision = decide_mute_transition(MuteState::NotMuting, false);
    assert_eq!(
        transition_decision,
        MuteTransitionDecision {
            next_state: MuteState::MutedByUs,
            action: MuteTransitionAction::SetMuted(true),
        }
    );
}

#[test]
fn decide_unmute_transition_from_muted_by_us_unmutes_and_resets_state() {
    let transition_decision = decide_unmute_transition(MuteState::MutedByUs);
    assert_eq!(
        transition_decision,
        MuteTransitionDecision {
            next_state: MuteState::NotMuting,
            action: MuteTransitionAction::SetMuted(false),
        }
    );
}

#[test]
fn decide_unmute_transition_from_user_muted_keeps_audio_muted_and_resets_state() {
    let transition_decision = decide_unmute_transition(MuteState::AudioWasAlreadyMutedByUser);
    assert_eq!(
        transition_decision,
        MuteTransitionDecision {
            next_state: MuteState::NotMuting,
            action: MuteTransitionAction::NoOp,
        }
    );
}

#[test]
fn mute_and_unmute_perform_expected_set_muted_calls() {
    let fake_controller_state = Arc::new(Mutex::new(FakeAudioControllerState::default()));
    let fake_controller = FakeAudioController::new(fake_controller_state.clone());
    let audio_mute_manager = AudioMuteManager::from_controller(Box::new(fake_controller));

    audio_mute_manager.mute().unwrap();
    audio_mute_manager.unmute().unwrap();

    let state_after_operations = fake_controller_state.lock().unwrap();
    assert_eq!(state_after_operations.set_muted_calls, vec![true, false]);
}

#[test]
fn mute_is_idempotent_when_already_muted_by_manager() {
    let fake_controller_state = Arc::new(Mutex::new(FakeAudioControllerState::default()));
    let fake_controller = FakeAudioController::new(fake_controller_state.clone());
    let audio_mute_manager = AudioMuteManager::from_controller(Box::new(fake_controller));

    audio_mute_manager.mute().unwrap();
    audio_mute_manager.mute().unwrap();

    let state_after_operations = fake_controller_state.lock().unwrap();
    assert_eq!(state_after_operations.set_muted_calls, vec![true]);
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
    assert_eq!(state_after_operations.set_muted_calls, Vec::<bool>::new());
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
    assert_eq!(state_after_operations.set_muted_calls, vec![true]);
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
    assert_eq!(state_after_drop.set_muted_calls, vec![true, false]);
}
