use super::{process_focus_snapshot_poll, ComparableActiveAppContext, FocusWatcherState};
use crate::active_app_context::{
    ActiveAppContextSnapshot, FocusConfidenceLevel, FocusEventSource, FocusedApplication,
    FocusedBrowserTab, FocusedWindow,
};
use std::time::{Duration, Instant};

fn build_active_app_context_snapshot_for_test(
    application_name: &str,
    window_title: &str,
    browser_tab_title: Option<&str>,
    browser_tab_origin: Option<&str>,
    confidence_level: FocusConfidenceLevel,
    captured_at: &str,
) -> ActiveAppContextSnapshot {
    ActiveAppContextSnapshot {
        focused_application: Some(FocusedApplication {
            display_name: application_name.to_string(),
            bundle_id: None,
            process_path: None,
        }),
        focused_window: Some(FocusedWindow {
            title: window_title.to_string(),
        }),
        focused_browser_tab: Some(FocusedBrowserTab {
            title: browser_tab_title.map(str::to_string),
            origin: browser_tab_origin.map(str::to_string),
            browser: None,
        }),
        event_source: FocusEventSource::Polling,
        confidence_level,
        captured_at: captured_at.to_string(),
    }
}

#[test]
fn comparable_active_app_context_is_stable_for_identical_snapshots() {
    let identical_focus_snapshot = build_active_app_context_snapshot_for_test(
        "Code",
        "notes.md",
        Some("Pull Request"),
        Some("https://example.com"),
        FocusConfidenceLevel::High,
        "2026-01-01T00:00:00Z",
    );

    let comparable_context_first = ComparableActiveAppContext::from(&identical_focus_snapshot);
    let comparable_context_second = ComparableActiveAppContext::from(&identical_focus_snapshot);

    assert_eq!(comparable_context_first, comparable_context_second);
}

#[test]
fn comparable_active_app_context_ignores_captured_at_changes() {
    let earlier_snapshot = build_active_app_context_snapshot_for_test(
        "Code",
        "notes.md",
        Some("Pull Request"),
        Some("https://example.com"),
        FocusConfidenceLevel::High,
        "2026-01-01T00:00:00Z",
    );
    let later_snapshot = build_active_app_context_snapshot_for_test(
        "Code",
        "notes.md",
        Some("Pull Request"),
        Some("https://example.com"),
        FocusConfidenceLevel::High,
        "2026-01-01T00:01:00Z",
    );

    assert_eq!(
        ComparableActiveAppContext::from(&earlier_snapshot),
        ComparableActiveAppContext::from(&later_snapshot)
    );
}

#[test]
fn comparable_active_app_context_changes_when_semantic_fields_change() {
    let original_snapshot = build_active_app_context_snapshot_for_test(
        "Code",
        "notes.md",
        Some("Pull Request"),
        Some("https://example.com"),
        FocusConfidenceLevel::High,
        "2026-01-01T00:00:00Z",
    );
    let changed_semantic_snapshot = build_active_app_context_snapshot_for_test(
        "Code",
        "notes.md",
        Some("Issue"),
        Some("https://example.org"),
        FocusConfidenceLevel::High,
        "2026-01-01T00:00:00Z",
    );

    assert_ne!(
        ComparableActiveAppContext::from(&original_snapshot),
        ComparableActiveAppContext::from(&changed_semantic_snapshot),
    );
}

#[test]
fn initial_observed_focus_snapshot_emits_after_debounce_window() {
    let debounce_window = Duration::from_millis(75);
    let initial_instant = Instant::now();
    let initial_state = FocusWatcherState::AwaitingInitialEmission;
    let focus_snapshot = build_active_app_context_snapshot_for_test(
        "Code",
        "notes.md",
        Some("Pull Request"),
        Some("https://example.com"),
        FocusConfidenceLevel::High,
        "2026-01-01T00:00:00Z",
    );

    let first_poll_result = process_focus_snapshot_poll(
        initial_state,
        focus_snapshot.clone(),
        initial_instant,
        debounce_window,
    );
    assert!(first_poll_result.emission_candidate.is_none());

    let second_poll_result = process_focus_snapshot_poll(
        first_poll_result.next_state,
        focus_snapshot,
        initial_instant + Duration::from_millis(250),
        debounce_window,
    );
    assert!(second_poll_result.emission_candidate.is_some());
}

#[test]
fn debounce_timer_resets_only_when_comparable_context_changes() {
    let debounce_window = Duration::from_millis(75);
    let initial_instant = Instant::now();
    let initial_state = FocusWatcherState::AwaitingInitialEmission;
    let first_focus_snapshot = build_active_app_context_snapshot_for_test(
        "Code",
        "notes.md",
        Some("Pull Request"),
        Some("https://example.com"),
        FocusConfidenceLevel::High,
        "2026-01-01T00:00:00Z",
    );
    let second_focus_snapshot = build_active_app_context_snapshot_for_test(
        "Terminal",
        "shell",
        None,
        None,
        FocusConfidenceLevel::High,
        "2026-01-01T00:00:01Z",
    );

    let first_poll_result = process_focus_snapshot_poll(
        initial_state,
        first_focus_snapshot.clone(),
        initial_instant,
        debounce_window,
    );
    assert!(first_poll_result.emission_candidate.is_none());

    let second_poll_result = process_focus_snapshot_poll(
        first_poll_result.next_state,
        second_focus_snapshot.clone(),
        initial_instant + Duration::from_millis(50),
        debounce_window,
    );
    assert!(second_poll_result.emission_candidate.is_none());

    let third_poll_result = process_focus_snapshot_poll(
        second_poll_result.next_state,
        second_focus_snapshot.clone(),
        initial_instant + Duration::from_millis(120),
        debounce_window,
    );
    assert!(third_poll_result.emission_candidate.is_none());

    let fourth_poll_result = process_focus_snapshot_poll(
        third_poll_result.next_state,
        second_focus_snapshot,
        initial_instant + Duration::from_millis(130),
        debounce_window,
    );
    assert!(fourth_poll_result.emission_candidate.is_some());
}

#[test]
fn stable_emitted_state_does_not_reemit_identical_context() {
    let debounce_window = Duration::from_millis(75);
    let base_instant = Instant::now();
    let stable_focus_snapshot = build_active_app_context_snapshot_for_test(
        "Code",
        "notes.md",
        Some("Pull Request"),
        Some("https://example.com"),
        FocusConfidenceLevel::High,
        "2026-01-01T00:00:00Z",
    );
    let stable_emitted_state = FocusWatcherState::StableEmitted {
        emitted_context: ComparableActiveAppContext::from(&stable_focus_snapshot),
    };
    let same_context_with_new_timestamp = build_active_app_context_snapshot_for_test(
        "Code",
        "notes.md",
        Some("Pull Request"),
        Some("https://example.com"),
        FocusConfidenceLevel::High,
        "2026-01-01T00:10:00Z",
    );

    let poll_result = process_focus_snapshot_poll(
        stable_emitted_state,
        same_context_with_new_timestamp,
        base_instant + Duration::from_millis(500),
        debounce_window,
    );

    assert!(poll_result.emission_candidate.is_none());
    assert!(matches!(
        poll_result.next_state,
        FocusWatcherState::StableEmitted { .. }
    ));
}

#[test]
fn debouncing_candidate_retries_after_emission_failure() {
    let debounce_window = Duration::from_millis(75);
    let base_instant = Instant::now();
    let initial_state = FocusWatcherState::AwaitingInitialEmission;
    let focus_snapshot = build_active_app_context_snapshot_for_test(
        "Code",
        "notes.md",
        Some("Pull Request"),
        Some("https://example.com/pr/123"),
        FocusConfidenceLevel::High,
        "2026-01-01T00:00:00Z",
    );

    let first_poll_result = process_focus_snapshot_poll(
        initial_state,
        focus_snapshot.clone(),
        base_instant,
        debounce_window,
    );
    assert!(first_poll_result.emission_candidate.is_none());

    let second_poll_result = process_focus_snapshot_poll(
        first_poll_result.next_state,
        focus_snapshot.clone(),
        base_instant + Duration::from_millis(100),
        debounce_window,
    );
    assert!(second_poll_result.emission_candidate.is_some());

    let third_poll_result = process_focus_snapshot_poll(
        second_poll_result.next_state,
        focus_snapshot,
        base_instant + Duration::from_millis(150),
        debounce_window,
    );
    assert!(third_poll_result.emission_candidate.is_some());
}
