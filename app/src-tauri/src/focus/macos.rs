use objc2::rc::Retained;
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_foundation::NSString;

use crate::focus::{
    FocusConfidenceLevel, FocusContextSnapshot, FocusEventSource, FocusTrackingCapabilities,
    FocusedApplication, FocusedBrowserTab,
};

fn get_frontmost_application() -> Option<Retained<NSRunningApplication>> {
    let workspace = NSWorkspace::sharedWorkspace();
    workspace.frontmostApplication()
}

fn nsstring_to_string(retained_string: &Retained<NSString>) -> String {
    retained_string.to_string()
}

pub fn get_current_focus_context() -> FocusContextSnapshot {
    let captured_at = chrono::Utc::now().to_rfc3339();

    let focused_application = get_frontmost_application().map(|application| {
        let display_name = application
            .localizedName()
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), nsstring_to_string);
        let bundle_id = application
            .bundleIdentifier()
            .as_ref()
            .map(nsstring_to_string);

        FocusedApplication {
            display_name,
            bundle_id,
            process_path: None,
        }
    });

    FocusContextSnapshot {
        focused_application,
        focused_window: None,
        focused_browser_tab: None::<FocusedBrowserTab>,
        event_source: FocusEventSource::Polling,
        confidence_level: FocusConfidenceLevel::Low,
        privacy_filtered: true,
        captured_at,
    }
}

pub fn get_focus_capabilities() -> FocusTrackingCapabilities {
    FocusTrackingCapabilities {
        supports_focused_application_detection: true,
        supports_focused_window_detection: false,
        supports_focused_browser_tab_detection: false,
        supports_realtime_event_streaming: true,
        supports_private_browsing_detection: false,
    }
}
