use super::{is_likely_browser_address_bar_candidate, supported_browser_from_application_name};

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
