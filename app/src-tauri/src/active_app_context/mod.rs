use serde::{Deserialize, Serialize};
use tauri::AppHandle;

#[cfg_attr(
    not(any(target_os = "windows", target_os = "macos", test)),
    allow(dead_code)
)]
mod shared;
mod watcher;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
mod windows;

pub use watcher::{start_focus_watcher, FocusWatcherHandle};

#[cfg_attr(not(any(target_os = "windows", target_os = "macos")), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedBrowser {
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    Safari,
    GoogleChrome,
    MicrosoftEdge,
    BraveBrowser,
    Arc,
    Firefox,
    Opera,
    Vivaldi,
    Chromium,
}

#[cfg_attr(not(any(target_os = "windows", target_os = "macos")), allow(dead_code))]
impl SupportedBrowser {
    pub fn display_name(self) -> &'static str {
        match self {
            SupportedBrowser::Safari => "Safari",
            SupportedBrowser::GoogleChrome => "Google Chrome",
            SupportedBrowser::MicrosoftEdge => "Microsoft Edge",
            SupportedBrowser::BraveBrowser => "Brave Browser",
            SupportedBrowser::Arc => "Arc",
            SupportedBrowser::Firefox => "Firefox",
            SupportedBrowser::Opera => "Opera",
            SupportedBrowser::Vivaldi => "Vivaldi",
            SupportedBrowser::Chromium => "Chromium",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusEventSource {
    Polling,
    Accessibility,
    Uia,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusConfidenceLevel {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FocusedApplication {
    pub display_name: String,
    pub bundle_id: Option<String>,
    pub process_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FocusedWindow {
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FocusedBrowserTab {
    pub title: Option<String>,
    pub origin: Option<String>,
    pub browser: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveAppContextSnapshot {
    pub focused_application: Option<FocusedApplication>,
    pub focused_window: Option<FocusedWindow>,
    pub focused_browser_tab: Option<FocusedBrowserTab>,
    pub event_source: FocusEventSource,
    pub confidence_level: FocusConfidenceLevel,
    pub captured_at: String,
}

pub fn get_current_active_app_context() -> ActiveAppContextSnapshot {
    #[cfg(target_os = "windows")]
    {
        windows::get_current_active_app_context()
    }
    #[cfg(target_os = "macos")]
    {
        macos::get_current_active_app_context()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        ActiveAppContextSnapshot {
            focused_application: None,
            focused_window: None,
            focused_browser_tab: None,
            event_source: FocusEventSource::Unknown,
            confidence_level: FocusConfidenceLevel::Low,
            captured_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

pub fn start_focus_watcher_in_app(app: &AppHandle) -> FocusWatcherHandle {
    start_focus_watcher(app.clone())
}

pub fn sync_focus_watcher_enabled(
    app: &AppHandle,
    focus_watcher_handle: &mut Option<FocusWatcherHandle>,
    send_active_app_context_enabled: bool,
) {
    if send_active_app_context_enabled {
        if focus_watcher_handle.is_none() {
            *focus_watcher_handle = Some(start_focus_watcher_in_app(app));
        }
    } else if let Some(existing_focus_watcher_handle) = focus_watcher_handle.take() {
        existing_focus_watcher_handle.stop();
    }
}

#[cfg(test)]
#[path = "../tests/active_app_context_shared_tests.rs"]
mod focus_shared_tests;
