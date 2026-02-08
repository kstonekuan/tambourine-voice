use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;

use windows::core::{BSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, RPC_E_CHANGED_MODE};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation, IUIAutomationCondition, IUIAutomationElement,
    IUIAutomationElementArray, IUIAutomationValuePattern, TreeScope_Subtree,
    UIA_ControlTypePropertyId, UIA_EditControlTypeId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

use super::shared::{
    determine_focus_confidence_level, infer_browser_tab_title_from_window_title,
    normalize_browser_document_origin, normalize_non_empty_focus_text,
};
use crate::active_app_context::{
    ActiveAppContextSnapshot, FocusConfidenceLevel, FocusEventSource, FocusedApplication,
    FocusedBrowserTab, FocusedWindow, SupportedBrowser,
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

fn supported_browser_from_application_name(application_name: &str) -> Option<SupportedBrowser> {
    let normalized_application_name = application_name.to_lowercase();
    match normalized_application_name.as_str() {
        "chrome" => Some(SupportedBrowser::GoogleChrome),
        "msedge" | "edge" => Some(SupportedBrowser::MicrosoftEdge),
        "brave" => Some(SupportedBrowser::BraveBrowser),
        "opera" | "opera_gx" => Some(SupportedBrowser::Opera),
        "arc" => Some(SupportedBrowser::Arc),
        "vivaldi" => Some(SupportedBrowser::Vivaldi),
        "chromium" => Some(SupportedBrowser::Chromium),
        "firefox" => Some(SupportedBrowser::Firefox),
        _ => None,
    }
}

fn bstr_to_non_empty_focus_text(raw_bstr: BSTR) -> Option<String> {
    let bstr_as_string = String::try_from(raw_bstr).ok()?;
    normalize_non_empty_focus_text(&bstr_as_string)
}

struct ComApartmentInitializationGuard {
    should_call_co_uninitialize_on_drop: bool,
}

impl ComApartmentInitializationGuard {
    fn initialize_single_threaded() -> Option<Self> {
        // SAFETY: CoInitializeEx is called once for this access path. The returned guard
        // tracks whether this call succeeded and should be balanced by CoUninitialize.
        let co_initialize_result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if co_initialize_result.is_ok() {
            Some(Self {
                should_call_co_uninitialize_on_drop: true,
            })
        } else if co_initialize_result == RPC_E_CHANGED_MODE {
            // COM is already initialized on this thread with another apartment model.
            // We can still use existing COM initialization, but must not uninitialize it here.
            Some(Self {
                should_call_co_uninitialize_on_drop: false,
            })
        } else {
            let co_initialize_result_as_u32 =
                u32::from_ne_bytes(co_initialize_result.0.to_ne_bytes());
            log::warn!(
                "Failed to initialize COM apartment for UI Automation: HRESULT=0x{co_initialize_result_as_u32:08X}"
            );
            None
        }
    }
}

impl Drop for ComApartmentInitializationGuard {
    fn drop(&mut self) {
        if self.should_call_co_uninitialize_on_drop {
            // SAFETY: This exactly balances a successful CoInitializeEx call made by this guard.
            unsafe { CoUninitialize() };
        }
    }
}

fn create_ui_automation_client() -> Option<(ComApartmentInitializationGuard, IUIAutomation)> {
    let com_apartment_initialization_guard =
        ComApartmentInitializationGuard::initialize_single_threaded()?;
    // SAFETY: COM has been initialized for this thread (or was already initialized).
    let ui_automation_client =
        unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }.ok()?;
    Some((com_apartment_initialization_guard, ui_automation_client))
}

fn is_likely_browser_address_bar_candidate(
    automation_id: Option<&str>,
    control_name: Option<&str>,
) -> bool {
    let normalized_automation_id = automation_id.map(str::to_lowercase);
    let normalized_control_name = control_name.map(str::to_lowercase);

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

fn get_automation_element_for_window(
    ui_automation_client: &IUIAutomation,
    window_handle: HWND,
) -> Option<IUIAutomationElement> {
    // SAFETY: The HWND is obtained from the OS. If it becomes invalid before this call,
    // UI Automation returns an error that we convert into None.
    unsafe { ui_automation_client.ElementFromHandle(window_handle) }.ok()
}

fn create_edit_control_type_condition(
    ui_automation_client: &IUIAutomation,
) -> Option<IUIAutomationCondition> {
    let edit_control_type_variant = VARIANT::from(UIA_EditControlTypeId.0);
    // SAFETY: The VARIANT points to stack data that remains valid for the duration of the call.
    unsafe {
        ui_automation_client
            .CreatePropertyCondition(UIA_ControlTypePropertyId, &edit_control_type_variant)
    }
    .ok()
}

fn find_subtree_elements_matching_condition(
    root_automation_element: &IUIAutomationElement,
    target_condition: &IUIAutomationCondition,
) -> Option<IUIAutomationElementArray> {
    // SAFETY: Both COM interfaces are owned, valid references provided by UI Automation.
    unsafe { root_automation_element.FindAll(TreeScope_Subtree, target_condition) }.ok()
}

fn get_automation_element_array_length(
    automation_element_array: &IUIAutomationElementArray,
) -> Option<i32> {
    // SAFETY: The element array interface comes directly from a successful UIA FindAll call.
    unsafe { automation_element_array.Length() }.ok()
}

fn get_automation_element_at_index(
    automation_element_array: &IUIAutomationElementArray,
    element_index: i32,
) -> Option<IUIAutomationElement> {
    // SAFETY: Index is bounded by a prior successful Length() result in caller iteration.
    unsafe { automation_element_array.GetElement(element_index) }.ok()
}

fn get_element_current_automation_id(automation_element: &IUIAutomationElement) -> Option<String> {
    // SAFETY: Reading a property from a valid UIA element; UIA enforces COM invariants.
    unsafe { automation_element.CurrentAutomationId() }
        .ok()
        .and_then(bstr_to_non_empty_focus_text)
}

fn get_element_current_name(automation_element: &IUIAutomationElement) -> Option<String> {
    // SAFETY: Reading a property from a valid UIA element; UIA enforces COM invariants.
    unsafe { automation_element.CurrentName() }
        .ok()
        .and_then(bstr_to_non_empty_focus_text)
}

fn get_value_pattern_for_edit_control(
    edit_control_element: &IUIAutomationElement,
) -> Option<IUIAutomationValuePattern> {
    // SAFETY: Pattern retrieval is performed on a UIA element returned by UIA enumeration.
    unsafe { edit_control_element.GetCurrentPatternAs(UIA_ValuePatternId) }.ok()
}

fn get_value_pattern_current_value(value_pattern: &IUIAutomationValuePattern) -> Option<String> {
    // SAFETY: Value pattern interface is obtained from a successful GetCurrentPatternAs call.
    unsafe { value_pattern.CurrentValue() }
        .ok()
        .and_then(bstr_to_non_empty_focus_text)
}

fn extract_normalized_origin_from_edit_control(
    edit_control_element: &IUIAutomationElement,
) -> Option<String> {
    let value_pattern = get_value_pattern_for_edit_control(edit_control_element)?;
    let raw_address_bar_value = get_value_pattern_current_value(&value_pattern);
    raw_address_bar_value
        .as_deref()
        .and_then(normalize_browser_document_origin)
}

fn extract_browser_document_origin_from_uia(hwnd: HWND) -> Option<String> {
    let (_com_apartment_initialization_guard, ui_automation_client) =
        create_ui_automation_client()?;
    let focused_window_automation_element =
        get_automation_element_for_window(&ui_automation_client, hwnd)?;
    let edit_control_type_condition = create_edit_control_type_condition(&ui_automation_client)?;
    let edit_control_elements = find_subtree_elements_matching_condition(
        &focused_window_automation_element,
        &edit_control_type_condition,
    )?;
    let edit_control_count = get_automation_element_array_length(&edit_control_elements)?;
    if edit_control_count <= 0 {
        return None;
    }

    for edit_control_index in 0..edit_control_count {
        let Some(edit_control_element) =
            get_automation_element_at_index(&edit_control_elements, edit_control_index)
        else {
            continue;
        };
        let automation_id = get_element_current_automation_id(&edit_control_element);
        let control_name = get_element_current_name(&edit_control_element);
        if !is_likely_browser_address_bar_candidate(
            automation_id.as_deref(),
            control_name.as_deref(),
        ) {
            continue;
        }
        if let Some(normalized_document_origin) =
            extract_normalized_origin_from_edit_control(&edit_control_element)
        {
            return Some(normalized_document_origin);
        }
    }

    None
}

pub fn get_current_active_app_context() -> ActiveAppContextSnapshot {
    let captured_at = chrono::Utc::now().to_rfc3339();

    let Some(hwnd) = get_foreground_window() else {
        return ActiveAppContextSnapshot {
            focused_application: None,
            focused_window: None,
            focused_browser_tab: None,
            event_source: FocusEventSource::Polling,
            confidence_level: FocusConfidenceLevel::Low,
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

    let supported_browser = focused_application
        .as_ref()
        .and_then(|focused_application| {
            supported_browser_from_application_name(&focused_application.display_name)
        });
    let browser_tab_title = supported_browser.and_then(|supported_browser| {
        infer_browser_tab_title_from_window_title(
            window_title.as_deref(),
            supported_browser.display_name(),
        )
    });
    let browser_document_origin =
        supported_browser.and_then(|_| extract_browser_document_origin_from_uia(hwnd));
    let focused_browser_tab = supported_browser.and_then(|supported_browser| {
        if browser_tab_title.is_none() && browser_document_origin.is_none() {
            return None;
        }

        Some(FocusedBrowserTab {
            title: browser_tab_title,
            origin: browser_document_origin,
            browser: Some(supported_browser.display_name().to_string()),
        })
    });
    let event_source = if focused_browser_tab
        .as_ref()
        .and_then(|focused_browser_tab| focused_browser_tab.origin.as_ref())
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
            .and_then(|focused_browser_tab| focused_browser_tab.origin.as_ref())
            .is_some(),
    );

    ActiveAppContextSnapshot {
        focused_application,
        focused_window,
        focused_browser_tab,
        event_source,
        confidence_level,
        captured_at,
    }
}

#[cfg(test)]
#[path = "../tests/active_app_context_windows_tests.rs"]
mod focus_windows_tests;
