use super::{
    browser_process_metadata_from_application_name, infer_browser_tab_title_from_window_title,
    is_likely_browser_address_bar_candidate, normalize_browser_document_origin,
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
fn browser_process_metadata_from_application_name_supports_chromium_and_firefox() {
    let chrome_metadata = browser_process_metadata_from_application_name("chrome")
        .expect("chrome should be recognized as a browser");
    assert_eq!(chrome_metadata.browser_display_name, "Google Chrome");
    assert!(chrome_metadata.supports_uia_address_bar);

    let firefox_metadata = browser_process_metadata_from_application_name("firefox")
        .expect("firefox should be recognized as a browser");
    assert_eq!(firefox_metadata.browser_display_name, "Firefox");
    assert!(!firefox_metadata.supports_uia_address_bar);

    assert!(browser_process_metadata_from_application_name("code").is_none());
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
    assert!(!is_likely_browser_address_bar_candidate(
        Some("searchResult"),
        Some("Find in page")
    ));
}
