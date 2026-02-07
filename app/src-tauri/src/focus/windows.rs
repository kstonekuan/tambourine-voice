use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

use crate::focus::{
    FocusConfidenceLevel, FocusContextSnapshot, FocusEventSource, FocusTrackingCapabilities,
    FocusedApplication, FocusedBrowserTab, FocusedWindow,
};

fn get_foreground_window() -> Option<HWND> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        None
    } else {
        Some(hwnd)
    }
}

fn get_window_title(hwnd: HWND) -> Option<String> {
    let mut buffer = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
    if len == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..len]))
}

fn get_process_path(hwnd: HWND) -> Option<String> {
    const MAX_PROCESS_PATH_UTF16_LENGTH: usize = 32_768;

    let mut process_id: u32 = 0;
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
            hwnd,
            Some(&mut process_id),
        );
    }
    if process_id == 0 {
        return None;
    }

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) };
    let handle = handle.ok()?;

    let mut buffer = vec![0u16; MAX_PROCESS_PATH_UTF16_LENGTH];
    let mut size = buffer.len() as u32;
    let process_path_result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    let close_process_handle_result = unsafe { CloseHandle(handle) };
    if let Err(close_process_handle_error) = close_process_handle_result {
        log::warn!(
            "Failed to close focused-window process handle after reading process path: {}",
            close_process_handle_error
        );
    }
    if process_path_result.is_err() || size == 0 {
        return None;
    }

    Some(
        OsString::from_wide(&buffer[..size as usize])
            .to_string_lossy()
            .to_string(),
    )
}

fn get_application_display_name(process_path: &str) -> String {
    Path::new(process_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(process_path)
        .to_string()
}

fn infer_browser_tab(window_title: &str, application_name: &str) -> Option<FocusedBrowserTab> {
    let known_browsers = [
        "chrome",
        "msedge",
        "brave",
        "opera",
        "firefox",
        "edge",
        "google chrome",
        "microsoft edge",
        "brave browser",
    ];

    let lower_app = application_name.to_lowercase();
    if !known_browsers.iter().any(|name| lower_app.contains(name)) {
        return None;
    }

    let mut title = window_title.to_string();
    if let Some(separator_index) = window_title.rfind(" - ") {
        title = window_title[..separator_index].to_string();
    }

    Some(FocusedBrowserTab {
        title: if title.is_empty() { None } else { Some(title) },
        url: None,
        browser: Some(application_name.to_string()),
    })
}

pub fn get_current_focus_context() -> FocusContextSnapshot {
    let captured_at = chrono::Utc::now().to_rfc3339();

    let hwnd = match get_foreground_window() {
        Some(hwnd) => hwnd,
        None => {
            return FocusContextSnapshot {
                focused_application: None,
                focused_window: None,
                focused_browser_tab: None,
                event_source: FocusEventSource::Polling,
                confidence_level: FocusConfidenceLevel::Low,
                privacy_filtered: true,
                captured_at,
            };
        }
    };

    let window_title = get_window_title(hwnd);
    let process_path = get_process_path(hwnd);

    let focused_application = process_path.as_ref().map(|path| FocusedApplication {
        display_name: get_application_display_name(path),
        bundle_id: None,
        process_path: Some(path.to_string()),
    });

    let focused_window = window_title.as_ref().map(|title| FocusedWindow {
        title: title.clone(),
    });

    let focused_browser_tab = match (window_title.as_ref(), focused_application.as_ref()) {
        (Some(title), Some(application)) => infer_browser_tab(title, &application.display_name),
        _ => None,
    };

    FocusContextSnapshot {
        focused_application,
        focused_window,
        focused_browser_tab,
        event_source: FocusEventSource::Polling,
        confidence_level: FocusConfidenceLevel::High,
        privacy_filtered: true,
        captured_at,
    }
}

pub fn get_focus_capabilities() -> FocusTrackingCapabilities {
    FocusTrackingCapabilities {
        supports_focused_application_detection: true,
        supports_focused_window_detection: true,
        supports_focused_browser_tab_detection: true,
        supports_realtime_event_streaming: true,
        supports_private_browsing_detection: false,
    }
}
