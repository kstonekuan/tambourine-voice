use super::MuteState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MuteTransitionAction {
    NoOp,
    SetMuted(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MuteTransitionDecision {
    pub(crate) next_state: MuteState,
    pub(crate) action: MuteTransitionAction,
}

pub(crate) fn decide_mute_transition(
    current_mute_state: MuteState,
    audio_is_currently_muted: bool,
) -> MuteTransitionDecision {
    match current_mute_state {
        MuteState::NotMuting => {
            if audio_is_currently_muted {
                MuteTransitionDecision {
                    next_state: MuteState::AudioWasAlreadyMutedByUser,
                    action: MuteTransitionAction::NoOp,
                }
            } else {
                MuteTransitionDecision {
                    next_state: MuteState::MutedByUs,
                    action: MuteTransitionAction::SetMuted(true),
                }
            }
        }
        MuteState::MutedByUs | MuteState::AudioWasAlreadyMutedByUser => MuteTransitionDecision {
            next_state: current_mute_state,
            action: MuteTransitionAction::NoOp,
        },
    }
}

pub(crate) fn decide_unmute_transition(current_mute_state: MuteState) -> MuteTransitionDecision {
    match current_mute_state {
        MuteState::MutedByUs => MuteTransitionDecision {
            next_state: MuteState::NotMuting,
            action: MuteTransitionAction::SetMuted(false),
        },
        MuteState::NotMuting | MuteState::AudioWasAlreadyMutedByUser => MuteTransitionDecision {
            next_state: MuteState::NotMuting,
            action: MuteTransitionAction::NoOp,
        },
    }
}
