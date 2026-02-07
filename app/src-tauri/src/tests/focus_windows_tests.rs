use super::{
    infer_browser_tab_title_from_window_title, is_likely_browser_address_bar_candidate,
    normalize_browser_document_origin, supported_browser_from_application_name,
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
fn supported_browser_from_application_name_supports_chromium_and_firefox() {
    let supported_chrome_browser = supported_browser_from_application_name("chrome")
        .expect("chrome should be recognized as a browser");
    assert_eq!(supported_chrome_browser.display_name(), "Google Chrome");

    let supported_firefox_browser = supported_browser_from_application_name("firefox")
        .expect("firefox should be recognized as a browser");
    assert_eq!(supported_firefox_browser.display_name(), "Firefox");

    assert!(supported_browser_from_application_name("code").is_none());
}

#[test]
fn infer_browser_tab_title_from_window_title_strips_browser_suffix() {
    let focused_window_title = Some("Focus Context Plan - Google Chrome");
    let inferred_tab_title =
        infer_browser_tab_title_from_window_title(focused_window_title, "Google Chrome");
    assert_eq!(inferred_tab_title.as_deref(), Some("Focus Context Plan"));
}

#[test]
fn is_likely_browser_address_bar_candidate_uses_automation_id_or_name_markers() {
    assert!(is_likely_browser_address_bar_candidate(
        Some("addressEditBox"),
        Some("Whatever")
    ));
    assert!(is_likely_browser_address_bar_candidate(
        None,
        Some("Address and search bar")
    ));
    assert!(is_likely_browser_address_bar_candidate(
        Some("urlbar-input"),
        Some("Search with Google or enter address")
    ));
    assert!(!is_likely_browser_address_bar_candidate(
        Some("searchResult"),
        Some("Find in page")
    ));
}
