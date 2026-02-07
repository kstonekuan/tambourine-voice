use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;

use windows::core::{BSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation, IUIAutomationElement, IUIAutomationValuePattern,
    TreeScope_Subtree, UIA_ControlTypePropertyId, UIA_EditControlTypeId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

use crate::focus::{
    FocusConfidenceLevel, FocusContextSnapshot, FocusEventSource, FocusedApplication,
    FocusedBrowserTab, FocusedWindow,
};

#[derive(Debug, Clone, Copy)]
struct BrowserProcessMetadata {
    browser_display_name: &'static str,
    supports_uia_address_bar: bool,
}

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
    let window_title_length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if window_title_length <= 0 {
        return None;
    }
    let window_title_length = usize::try_from(window_title_length).ok()?;
    Some(String::from_utf16_lossy(&buffer[..window_title_length]))
}

fn get_process_path(hwnd: HWND) -> Option<String> {
    const MAX_PROCESS_PATH_UTF16_LENGTH: usize = 32_768;

    let mut process_id: u32 = 0;
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
            hwnd,
            Some(&raw mut process_id),
        );
    }
    if process_id == 0 {
        return None;
    }

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) };
    let handle = handle.ok()?;

    let mut buffer = vec![0u16; MAX_PROCESS_PATH_UTF16_LENGTH];
    let mut size = u32::try_from(buffer.len()).ok()?;
    let process_path_result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &raw mut size,
        )
    };
    let close_process_handle_result = unsafe { CloseHandle(handle) };
    if let Err(close_process_handle_error) = close_process_handle_result {
        log::warn!(
            "Failed to close focused-window process handle after reading process path: {close_process_handle_error}"
        );
    }
    if process_path_result.is_err() || size == 0 {
        return None;
    }

    let process_path_length = usize::try_from(size).ok()?;
    Some(
        OsString::from_wide(&buffer[..process_path_length])
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

fn normalize_non_empty_focus_text(raw_focus_text: &str) -> Option<String> {
    let trimmed_focus_text = raw_focus_text.trim();
    if trimmed_focus_text.is_empty() {
        None
    } else {
        Some(trimmed_focus_text.to_string())
    }
}

fn normalize_browser_document_url(raw_document_url: &str) -> Option<String> {
    let trimmed_document_url = normalize_non_empty_focus_text(raw_document_url)?;
    let scheme_separator_index = trimmed_document_url.find("://")?;
    let (url_scheme, url_remainder_with_separator) =
        trimmed_document_url.split_at(scheme_separator_index);
    let url_remainder = &url_remainder_with_separator[3..];
    if url_scheme.is_empty() || url_remainder.is_empty() {
        return None;
    }

    let authority_end_index = url_remainder
        .find(['/', '?', '#'])
        .unwrap_or(url_remainder.len());
    let authority_component = &url_remainder[..authority_end_index];
    if authority_component.is_empty() {
        return None;
    }

    let path_with_possible_query_or_fragment = if authority_end_index < url_remainder.len()
        && url_remainder.as_bytes()[authority_end_index] == b'/'
    {
        &url_remainder[authority_end_index..]
    } else {
        ""
    };
    let path_without_query_or_fragment = path_with_possible_query_or_fragment
        .split(['?', '#'])
        .next()
        .unwrap_or("");

    Some(format!(
        "{url_scheme}://{authority_component}{path_without_query_or_fragment}"
    ))
}

fn browser_process_metadata_from_application_name(
    application_name: &str,
) -> Option<BrowserProcessMetadata> {
    let normalized_application_name = application_name.to_lowercase();
    match normalized_application_name.as_str() {
        "chrome" => Some(BrowserProcessMetadata {
            browser_display_name: "Google Chrome",
            supports_uia_address_bar: true,
        }),
        "msedge" | "edge" => Some(BrowserProcessMetadata {
            browser_display_name: "Microsoft Edge",
            supports_uia_address_bar: true,
        }),
        "brave" => Some(BrowserProcessMetadata {
            browser_display_name: "Brave Browser",
            supports_uia_address_bar: true,
        }),
        "opera" | "opera_gx" => Some(BrowserProcessMetadata {
            browser_display_name: "Opera",
            supports_uia_address_bar: true,
        }),
        "arc" => Some(BrowserProcessMetadata {
            browser_display_name: "Arc",
            supports_uia_address_bar: true,
        }),
        "vivaldi" => Some(BrowserProcessMetadata {
            browser_display_name: "Vivaldi",
            supports_uia_address_bar: true,
        }),
        "chromium" => Some(BrowserProcessMetadata {
            browser_display_name: "Chromium",
            supports_uia_address_bar: true,
        }),
        "firefox" => Some(BrowserProcessMetadata {
            browser_display_name: "Firefox",
            supports_uia_address_bar: false,
        }),
        _ => None,
    }
}

fn infer_browser_tab_title_from_window_title(
    focused_window_title: Option<&str>,
    browser_name: &str,
) -> Option<String> {
    let focused_window_title = normalize_non_empty_focus_text(focused_window_title?)?;
    for title_separator in [" - ", " — "] {
        let browser_suffix = format!("{title_separator}{browser_name}");
        if let Some(raw_tab_title) = focused_window_title.strip_suffix(&browser_suffix) {
            return normalize_non_empty_focus_text(raw_tab_title)
                .or_else(|| Some(focused_window_title.clone()));
        }
    }

    Some(focused_window_title)
}

fn bstr_to_non_empty_focus_text(raw_bstr: BSTR) -> Option<String> {
    let bstr_as_string = raw_bstr.to_string().ok()?;
    normalize_non_empty_focus_text(&bstr_as_string)
}

fn create_ui_automation_client() -> Option<IUIAutomation> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).ok()
    }
}

fn is_likely_browser_address_bar_candidate(
    automation_id: Option<&str>,
    control_name: Option<&str>,
) -> bool {
    let normalized_automation_id = automation_id.map(|automation_id| automation_id.to_lowercase());
    let normalized_control_name = control_name.map(|control_name| control_name.to_lowercase());

    let automation_id_contains_address_bar_marker = normalized_automation_id
        .as_deref()
        .is_some_and(|automation_id| {
            ["address", "searchbox", "urlbar", "omnibox"]
                .iter()
                .any(|marker| automation_id.contains(marker))
        });
    if automation_id_contains_address_bar_marker {
        return true;
    }

    normalized_control_name
        .as_deref()
        .is_some_and(|control_name| {
            [
                "address and search bar",
                "search or enter address",
                "search with google or enter address",
                "address bar",
            ]
            .iter()
            .any(|marker| control_name.contains(marker))
        })
}

fn extract_normalized_url_from_edit_control(
    edit_control_element: &IUIAutomationElement,
) -> Option<String> {
    let value_pattern: IUIAutomationValuePattern =
        unsafe { edit_control_element.GetCurrentPatternAs(UIA_ValuePatternId.0) }.ok()?;
    let raw_current_value = value_pattern.CurrentValue().ok()?;
    let address_bar_value = bstr_to_non_empty_focus_text(raw_current_value)?;
    normalize_browser_document_url(&address_bar_value)
}

fn extract_browser_document_url_from_uia(hwnd: HWND) -> Option<String> {
    let ui_automation_client = create_ui_automation_client()?;
    let focused_window_automation_element =
        unsafe { ui_automation_client.ElementFromHandle(hwnd) }.ok()?;
    let edit_control_type_condition = ui_automation_client
        .CreatePropertyCondition(
            UIA_ControlTypePropertyId,
            VARIANT::from(UIA_EditControlTypeId.0),
        )
        .ok()?;
    let edit_control_elements = unsafe {
        focused_window_automation_element.FindAll(TreeScope_Subtree, &edit_control_type_condition)
    }
    .ok()?;
    let edit_control_count = edit_control_elements.Length().ok()?;
    for edit_control_index in 0..edit_control_count {
        let edit_control_element =
            unsafe { edit_control_elements.GetElement(edit_control_index) }.ok()?;
        let automation_id = edit_control_element
            .CurrentAutomationId()
            .ok()
            .and_then(bstr_to_non_empty_focus_text);
        let control_name = edit_control_element
            .CurrentName()
            .ok()
            .and_then(bstr_to_non_empty_focus_text);
        if !is_likely_browser_address_bar_candidate(
            automation_id.as_deref(),
            control_name.as_deref(),
        ) {
            continue;
        }

        if let Some(normalized_document_url) =
            extract_normalized_url_from_edit_control(&edit_control_element)
        {
            return Some(normalized_document_url);
        }
    }

    None
}

fn determine_focus_confidence_level(
    focused_window_is_present: bool,
    focused_browser_tab_is_present: bool,
    focused_browser_url_is_present: bool,
) -> FocusConfidenceLevel {
    if focused_window_is_present && focused_browser_url_is_present {
        FocusConfidenceLevel::High
    } else if focused_window_is_present || focused_browser_tab_is_present {
        FocusConfidenceLevel::Medium
    } else {
        FocusConfidenceLevel::Low
    }
}

pub fn get_current_focus_context() -> FocusContextSnapshot {
    let captured_at = chrono::Utc::now().to_rfc3339();

    let Some(hwnd) = get_foreground_window() else {
        return FocusContextSnapshot {
            focused_application: None,
            focused_window: None,
            focused_browser_tab: None,
            event_source: FocusEventSource::Polling,
            confidence_level: FocusConfidenceLevel::Low,
            privacy_filtered: true,
            captured_at,
        };
    };

    let window_title = get_window_title(hwnd);
    let process_path = get_process_path(hwnd);

    let focused_application = process_path.as_ref().map(|path| FocusedApplication {
        display_name: get_application_display_name(path),
        bundle_id: None,
        process_path: Some(path.clone()),
    });

    let focused_window = window_title.as_ref().map(|title| FocusedWindow {
        title: title.clone(),
    });

    let browser_process_metadata = focused_application.as_ref().and_then(|application| {
        browser_process_metadata_from_application_name(&application.display_name)
    });
    let browser_tab_title = browser_process_metadata.and_then(|browser_process_metadata| {
        infer_browser_tab_title_from_window_title(
            window_title.as_deref(),
            browser_process_metadata.browser_display_name,
        )
    });
    let browser_document_url = browser_process_metadata
        .filter(|browser_process_metadata| browser_process_metadata.supports_uia_address_bar)
        .and_then(|_| extract_browser_document_url_from_uia(hwnd));
    let focused_browser_tab = browser_process_metadata.and_then(|browser_process_metadata| {
        if browser_tab_title.is_none() && browser_document_url.is_none() {
            return None;
        }

        Some(FocusedBrowserTab {
            title: browser_tab_title,
            url: browser_document_url,
            browser: Some(browser_process_metadata.browser_display_name.to_string()),
        })
    });
    let event_source = if focused_browser_tab
        .as_ref()
        .and_then(|focused_browser_tab| focused_browser_tab.url.as_ref())
        .is_some()
    {
        FocusEventSource::Uia
    } else {
        FocusEventSource::Polling
    };
    let confidence_level = determine_focus_confidence_level(
        focused_window.is_some(),
        focused_browser_tab.is_some(),
        focused_browser_tab
            .as_ref()
            .and_then(|focused_browser_tab| focused_browser_tab.url.as_ref())
            .is_some(),
    );

    FocusContextSnapshot {
        focused_application,
        focused_window,
        focused_browser_tab,
        event_source,
        confidence_level,
        privacy_filtered: true,
        captured_at,
    }
}

#[cfg(test)]
#[path = "../tests/focus_windows_tests.rs"]
mod focus_windows_tests;
