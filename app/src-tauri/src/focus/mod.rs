use serde::{Deserialize, Serialize};
use tauri::AppHandle;

mod watcher;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
mod windows;

pub use watcher::{start_focus_watcher, FocusWatcherHandle};

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
    pub url: Option<String>,
    pub browser: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FocusContextSnapshot {
    pub focused_application: Option<FocusedApplication>,
    pub focused_window: Option<FocusedWindow>,
    pub focused_browser_tab: Option<FocusedBrowserTab>,
    pub event_source: FocusEventSource,
    pub confidence_level: FocusConfidenceLevel,
    pub privacy_filtered: bool,
    pub captured_at: String,
}

pub fn get_current_focus_context() -> FocusContextSnapshot {
    #[cfg(target_os = "windows")]
    {
        windows::get_current_focus_context()
    }
    #[cfg(target_os = "macos")]
    {
        macos::get_current_focus_context()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        FocusContextSnapshot {
            focused_application: None,
            focused_window: None,
            focused_browser_tab: None,
            event_source: FocusEventSource::Unknown,
            confidence_level: FocusConfidenceLevel::Low,
            privacy_filtered: true,
            captured_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

pub fn start_focus_watcher_in_app(app: &AppHandle) -> FocusWatcherHandle {
    start_focus_watcher(app.clone())
}
