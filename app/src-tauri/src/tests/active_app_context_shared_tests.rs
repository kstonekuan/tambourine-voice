use super::shared::{
    determine_focus_confidence_level, infer_browser_tab_title_from_window_title,
    normalize_browser_document_origin, normalize_non_empty_focus_text,
};
use super::FocusConfidenceLevel;

#[test]
fn normalize_non_empty_focus_text_returns_none_for_whitespace() {
    assert_eq!(normalize_non_empty_focus_text("   \n\t"), None);
}

#[test]
fn normalize_browser_document_origin_removes_path_query_and_fragment() {
    let raw_document_url = "https://example.com/path/to/page?token=abc#section";
    let normalized_document_origin = normalize_browser_document_origin(raw_document_url);
    assert_eq!(
        normalized_document_origin.as_deref(),
        Some("https://example.com")
    );
}

#[test]
fn normalize_browser_document_origin_keeps_origin_when_path_is_missing() {
    let raw_document_url = "https://example.com?token=abc";
    let normalized_document_origin = normalize_browser_document_origin(raw_document_url);
    assert_eq!(
        normalized_document_origin.as_deref(),
        Some("https://example.com")
    );
}

#[test]
fn normalize_browser_document_origin_supports_scheme_less_browser_values() {
    let raw_document_url = "github.com/kstonekuan/tambourine-voice";
    let normalized_document_origin = normalize_browser_document_origin(raw_document_url);
    assert_eq!(
        normalized_document_origin.as_deref(),
        Some("https://github.com")
    );
}

#[test]
fn normalize_browser_document_origin_rejects_non_url_like_search_queries() {
    let raw_document_url = "how to fix rust clippy warning";
    let normalized_document_origin = normalize_browser_document_origin(raw_document_url);
    assert_eq!(normalized_document_origin, None);
}

#[test]
fn infer_browser_tab_title_from_window_title_strips_browser_suffix() {
    let focused_window_title = Some("Active App Context Plan - Google Chrome");
    let inferred_tab_title =
        infer_browser_tab_title_from_window_title(focused_window_title, "Google Chrome");
    assert_eq!(
        inferred_tab_title.as_deref(),
        Some("Active App Context Plan")
    );
}

#[test]
fn determine_focus_confidence_level_prioritizes_origin_signal() {
    assert_eq!(
        determine_focus_confidence_level(true, true, true),
        FocusConfidenceLevel::High
    );
    assert_eq!(
        determine_focus_confidence_level(true, false, false),
        FocusConfidenceLevel::Medium
    );
    assert_eq!(
        determine_focus_confidence_level(false, false, false),
        FocusConfidenceLevel::Low
    );
}
