use super::{
    browser_name_from_bundle_identifier, infer_browser_tab_title_from_window_title,
    normalize_browser_document_origin,
};

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
fn browser_name_from_bundle_identifier_supports_v1_browser_set() {
    assert_eq!(
        browser_name_from_bundle_identifier("com.apple.Safari"),
        Some("Safari")
    );
    assert_eq!(
        browser_name_from_bundle_identifier("com.google.Chrome"),
        Some("Google Chrome")
    );
    assert_eq!(
        browser_name_from_bundle_identifier("com.microsoft.edgemac"),
        Some("Microsoft Edge")
    );
    assert_eq!(
        browser_name_from_bundle_identifier("com.brave.Browser"),
        Some("Brave Browser")
    );
    assert_eq!(
        browser_name_from_bundle_identifier("company.thebrowser.Browser"),
        Some("Arc")
    );
    assert_eq!(
        browser_name_from_bundle_identifier("org.mozilla.firefox"),
        None
    );
}

#[test]
fn infer_browser_tab_title_from_window_title_strips_browser_suffix() {
    let focused_window_title = Some("Focus Context Plan - Google Chrome");
    let inferred_tab_title =
        infer_browser_tab_title_from_window_title(focused_window_title, "Google Chrome");
    assert_eq!(inferred_tab_title.as_deref(), Some("Focus Context Plan"));
}
