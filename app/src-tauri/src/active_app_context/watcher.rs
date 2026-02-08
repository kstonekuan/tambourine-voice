#[cfg(target_os = "macos")]
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use crate::active_app_context::{
    get_current_active_app_context, ActiveAppContextSnapshot, FocusConfidenceLevel,
};
use crate::events::EventName;

#[derive(Debug, Clone)]
pub struct FocusWatcherHandle {
    should_stop: Arc<AtomicBool>,
}

impl FocusWatcherHandle {
    pub fn stop(&self) {
        self.should_stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for FocusWatcherHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ComparableActiveAppContext {
    focused_application_display_name: Option<String>,
    focused_window_title: Option<String>,
    focused_browser_tab_title: Option<String>,
    focused_browser_tab_origin: Option<String>,
    confidence_level: FocusConfidenceLevel,
}

impl From<&ActiveAppContextSnapshot> for ComparableActiveAppContext {
    fn from(snapshot: &ActiveAppContextSnapshot) -> Self {
        Self {
            focused_application_display_name: snapshot
                .focused_application
                .as_ref()
                .map(|application| application.display_name.clone()),
            focused_window_title: snapshot
                .focused_window
                .as_ref()
                .map(|window| window.title.clone()),
            focused_browser_tab_title: snapshot
                .focused_browser_tab
                .as_ref()
                .and_then(|browser_tab| browser_tab.title.clone()),
            focused_browser_tab_origin: snapshot
                .focused_browser_tab
                .as_ref()
                .and_then(|browser_tab| browser_tab.origin.clone()),
            confidence_level: snapshot.confidence_level,
        }
    }
}

#[derive(Debug, Clone)]
struct DebouncingCandidateState {
    candidate_context: ComparableActiveAppContext,
    candidate_snapshot: ActiveAppContextSnapshot,
    first_seen_at: Instant,
}

#[derive(Debug, Clone)]
struct EmissionCandidate {
    candidate_context: ComparableActiveAppContext,
    candidate_snapshot: ActiveAppContextSnapshot,
}

#[derive(Debug, Clone)]
struct WatcherPollResult {
    next_state: FocusWatcherState,
    emission_candidate: Option<EmissionCandidate>,
}

#[derive(Debug, Clone)]
enum FocusWatcherState {
    AwaitingInitialEmission,
    StableEmitted {
        emitted_context: ComparableActiveAppContext,
    },
    DebouncingCandidate(Box<DebouncingCandidateState>),
}

fn process_focus_snapshot_poll(
    current_state: FocusWatcherState,
    polled_snapshot: ActiveAppContextSnapshot,
    observed_at: Instant,
    debounce_window: Duration,
) -> WatcherPollResult {
    let polled_context = ComparableActiveAppContext::from(&polled_snapshot);

    match current_state {
        FocusWatcherState::AwaitingInitialEmission => WatcherPollResult {
            next_state: FocusWatcherState::DebouncingCandidate(Box::new(
                DebouncingCandidateState {
                    candidate_context: polled_context,
                    candidate_snapshot: polled_snapshot,
                    first_seen_at: observed_at,
                },
            )),
            emission_candidate: None,
        },
        FocusWatcherState::StableEmitted { emitted_context } => {
            if emitted_context == polled_context {
                WatcherPollResult {
                    next_state: FocusWatcherState::StableEmitted { emitted_context },
                    emission_candidate: None,
                }
            } else {
                WatcherPollResult {
                    next_state: FocusWatcherState::DebouncingCandidate(Box::new(
                        DebouncingCandidateState {
                            candidate_context: polled_context,
                            candidate_snapshot: polled_snapshot,
                            first_seen_at: observed_at,
                        },
                    )),
                    emission_candidate: None,
                }
            }
        }
        FocusWatcherState::DebouncingCandidate(mut debouncing_candidate_state) => {
            if debouncing_candidate_state.candidate_context == polled_context {
                debouncing_candidate_state.candidate_snapshot = polled_snapshot;
            } else {
                debouncing_candidate_state = Box::new(DebouncingCandidateState {
                    candidate_context: polled_context,
                    candidate_snapshot: polled_snapshot,
                    first_seen_at: observed_at,
                });
            }

            let elapsed_since_first_seen =
                observed_at.saturating_duration_since(debouncing_candidate_state.first_seen_at);
            let emission_candidate = if elapsed_since_first_seen >= debounce_window {
                Some(EmissionCandidate {
                    candidate_context: debouncing_candidate_state.candidate_context.clone(),
                    candidate_snapshot: debouncing_candidate_state.candidate_snapshot.clone(),
                })
            } else {
                None
            };

            WatcherPollResult {
                next_state: FocusWatcherState::DebouncingCandidate(debouncing_candidate_state),
                emission_candidate,
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn get_active_app_context_snapshot_thread_safe(
    app: &AppHandle,
) -> Option<ActiveAppContextSnapshot> {
    let (snapshot_sender, snapshot_receiver) = mpsc::sync_channel::<ActiveAppContextSnapshot>(1);

    app.run_on_main_thread(move || {
        let snapshot = get_current_active_app_context();
        let _ = snapshot_sender.send(snapshot);
    })
    .ok()?;

    snapshot_receiver
        .recv_timeout(Duration::from_millis(150))
        .ok()
}

#[cfg(not(target_os = "macos"))]
fn get_active_app_context_snapshot_thread_safe(
    _app: &AppHandle,
) -> Option<ActiveAppContextSnapshot> {
    Some(get_current_active_app_context())
}

pub fn start_focus_watcher(app: AppHandle) -> FocusWatcherHandle {
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = should_stop.clone();

    thread::spawn(move || {
        let poll_interval = Duration::from_millis(250);
        let debounce_window = Duration::from_millis(75);
        let mut focus_watcher_state = FocusWatcherState::AwaitingInitialEmission;

        while !should_stop_clone.load(Ordering::SeqCst) {
            let Some(snapshot) = get_active_app_context_snapshot_thread_safe(&app) else {
                thread::sleep(poll_interval);
                continue;
            };
            let watcher_poll_result = process_focus_snapshot_poll(
                focus_watcher_state,
                snapshot,
                Instant::now(),
                debounce_window,
            );
            focus_watcher_state = watcher_poll_result.next_state;

            match watcher_poll_result.emission_candidate {
                Some(emission_candidate)
                    if app
                        .emit(
                            EventName::ActiveAppContextChanged.as_str(),
                            &emission_candidate.candidate_snapshot,
                        )
                        .is_ok() =>
                {
                    focus_watcher_state = FocusWatcherState::StableEmitted {
                        emitted_context: emission_candidate.candidate_context,
                    };
                }
                Some(_) | None => {}
            }

            thread::sleep(poll_interval);
        }
    });

    FocusWatcherHandle { should_stop }
}

#[cfg(test)]
#[path = "../tests/watcher_tests.rs"]
mod watcher_tests;
