use super::supported_browser_from_bundle_identifier;
use crate::active_app_context::SupportedBrowser;

#[test]
fn supported_browser_from_bundle_identifier_supports_v1_browser_set() {
    assert_eq!(
        supported_browser_from_bundle_identifier("com.apple.Safari")
            .map(SupportedBrowser::display_name),
        Some("Safari")
    );
    assert_eq!(
        supported_browser_from_bundle_identifier("com.google.Chrome")
            .map(SupportedBrowser::display_name),
        Some("Google Chrome")
    );
    assert_eq!(
        supported_browser_from_bundle_identifier("com.microsoft.edgemac")
            .map(SupportedBrowser::display_name),
        Some("Microsoft Edge")
    );
    assert_eq!(
        supported_browser_from_bundle_identifier("com.brave.Browser")
            .map(SupportedBrowser::display_name),
        Some("Brave Browser")
    );
    assert_eq!(
        supported_browser_from_bundle_identifier("company.thebrowser.Browser")
            .map(SupportedBrowser::display_name),
        Some("Arc")
    );
    assert_eq!(
        supported_browser_from_bundle_identifier("org.mozilla.firefox")
            .map(SupportedBrowser::display_name),
        Some("Firefox")
    );
    assert_eq!(
        supported_browser_from_bundle_identifier("com.operasoftware.Opera")
            .map(SupportedBrowser::display_name),
        Some("Opera")
    );
    assert_eq!(
        supported_browser_from_bundle_identifier("com.vivaldi.Vivaldi")
            .map(SupportedBrowser::display_name),
        Some("Vivaldi")
    );
    assert_eq!(
        supported_browser_from_bundle_identifier("org.chromium.Chromium")
            .map(SupportedBrowser::display_name),
        Some("Chromium")
    );
}
