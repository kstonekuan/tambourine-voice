use std::ffi::c_void;
use std::ptr;

use core_foundation::base::{CFType, CFTypeID, CFTypeRef, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowLayer, kCGWindowListExcludeDesktopElements,
    kCGWindowListOptionOnScreenOnly, kCGWindowName, kCGWindowOwnerPID,
};
use objc2::rc::Retained;
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_foundation::NSString;

use crate::focus::{
    FocusConfidenceLevel, FocusContextSnapshot, FocusEventSource, FocusedApplication,
    FocusedBrowserTab, FocusedWindow,
};

type AccessibilityUiElementRef = *const c_void;
type AccessibilityError = i32;

const ACCESSIBILITY_SUCCESS: AccessibilityError = 0;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementGetTypeID() -> CFTypeID;
    fn AXUIElementCreateApplication(processIdentifier: i32) -> AccessibilityUiElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AccessibilityUiElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AccessibilityError;
}

#[derive(Debug, Clone)]
struct FrontmostApplicationMetadata {
    display_name: String,
    bundle_identifier: Option<String>,
    process_identifier: i32,
}

#[derive(Debug, Clone, Default)]
struct AccessibilityFocusedWindowDetails {
    focused_window_title: Option<String>,
    focused_document_url: Option<String>,
}

fn get_frontmost_application() -> Option<Retained<NSRunningApplication>> {
    let workspace = NSWorkspace::sharedWorkspace();
    workspace.frontmostApplication()
}

fn nsstring_to_string(retained_string: &Retained<NSString>) -> String {
    retained_string.to_string()
}

fn normalize_non_empty_focus_text(raw_focus_text: &str) -> Option<String> {
    let trimmed_focus_text = raw_focus_text.trim();
    if trimmed_focus_text.is_empty() {
        None
    } else {
        Some(trimmed_focus_text.to_string())
    }
}

fn browser_name_from_bundle_identifier(bundle_identifier: &str) -> Option<&'static str> {
    match bundle_identifier {
        "com.apple.Safari" => Some("Safari"),
        "com.google.Chrome" => Some("Google Chrome"),
        "com.microsoft.edgemac" => Some("Microsoft Edge"),
        "com.brave.Browser" => Some("Brave Browser"),
        "company.thebrowser.Browser" => Some("Arc"),
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

fn normalize_browser_document_origin(raw_document_url: &str) -> Option<String> {
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

    Some(format!("{url_scheme}://{authority_component}"))
}

fn is_accessibility_api_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

fn create_accessibility_application_element(process_identifier: i32) -> Option<CFType> {
    let raw_accessibility_element =
        unsafe { AXUIElementCreateApplication(process_identifier) } as CFTypeRef;
    if raw_accessibility_element.is_null() {
        None
    } else {
        Some(unsafe { CFType::wrap_under_create_rule(raw_accessibility_element) })
    }
}

fn copy_accessibility_attribute_value(
    accessibility_element: &CFType,
    attribute_name: &str,
) -> Option<CFType> {
    let accessibility_ui_element_type_identifier = unsafe { AXUIElementGetTypeID() };
    if accessibility_element.type_of() != accessibility_ui_element_type_identifier {
        return None;
    }

    let accessibility_attribute_name = CFString::new(attribute_name);
    let mut raw_attribute_value: CFTypeRef = ptr::null();
    let attribute_copy_status = unsafe {
        AXUIElementCopyAttributeValue(
            accessibility_element.as_CFTypeRef() as AccessibilityUiElementRef,
            accessibility_attribute_name.as_concrete_TypeRef(),
            &raw mut raw_attribute_value,
        )
    };
    if attribute_copy_status != ACCESSIBILITY_SUCCESS || raw_attribute_value.is_null() {
        return None;
    }

    Some(unsafe { CFType::wrap_under_create_rule(raw_attribute_value) })
}

fn copy_accessibility_element_attribute_value(
    accessibility_element: &CFType,
    attribute_name: &str,
) -> Option<CFType> {
    let accessibility_attribute_value =
        copy_accessibility_attribute_value(accessibility_element, attribute_name)?;
    let accessibility_ui_element_type_identifier = unsafe { AXUIElementGetTypeID() };
    if accessibility_attribute_value.type_of() == accessibility_ui_element_type_identifier {
        Some(accessibility_attribute_value)
    } else {
        None
    }
}

fn copy_accessibility_string_attribute_value(
    accessibility_element: &CFType,
    attribute_name: &str,
) -> Option<String> {
    let accessibility_attribute_value =
        copy_accessibility_attribute_value(accessibility_element, attribute_name)?;
    let accessibility_attribute_string = accessibility_attribute_value.downcast::<CFString>()?;
    normalize_non_empty_focus_text(&accessibility_attribute_string.to_string())
}

fn get_focused_window_accessibility_element(
    application_accessibility_element: &CFType,
) -> Option<CFType> {
    copy_accessibility_element_attribute_value(application_accessibility_element, "AXFocusedWindow")
        .or_else(|| {
            copy_accessibility_element_attribute_value(
                application_accessibility_element,
                "AXFocusedUIElement",
            )
        })
}

fn get_accessibility_focused_window_details(
    process_identifier: i32,
) -> Option<AccessibilityFocusedWindowDetails> {
    let application_accessibility_element =
        create_accessibility_application_element(process_identifier)?;
    let focused_window_accessibility_element =
        get_focused_window_accessibility_element(&application_accessibility_element)?;

    Some(AccessibilityFocusedWindowDetails {
        focused_window_title: copy_accessibility_string_attribute_value(
            &focused_window_accessibility_element,
            "AXTitle",
        ),
        focused_document_url: copy_accessibility_string_attribute_value(
            &focused_window_accessibility_element,
            "AXDocument",
        ),
    })
}

fn extract_window_dictionary_number_field(
    typed_window_dictionary: &CFDictionary<CFString, CFType>,
    key_name: &CFString,
) -> Option<i32> {
    let dictionary_value = typed_window_dictionary.find(key_name)?;
    let number_value = dictionary_value.downcast::<CFNumber>()?;
    number_value.to_i32()
}

fn extract_window_dictionary_string_field(
    typed_window_dictionary: &CFDictionary<CFString, CFType>,
    key_name: &CFString,
) -> Option<String> {
    let dictionary_value = typed_window_dictionary.find(key_name)?;
    let string_value = dictionary_value.downcast::<CFString>()?;
    normalize_non_empty_focus_text(&string_value.to_string())
}

fn get_frontmost_window_title_from_core_graphics(
    frontmost_process_identifier: i32,
) -> Option<String> {
    let window_info_array = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    )?;

    let owner_process_identifier_key = unsafe { CFString::wrap_under_get_rule(kCGWindowOwnerPID) };
    let window_layer_key = unsafe { CFString::wrap_under_get_rule(kCGWindowLayer) };
    let window_title_key = unsafe { CFString::wrap_under_get_rule(kCGWindowName) };

    for untyped_window_dictionary_value in window_info_array.iter() {
        let raw_window_dictionary_pointer = *untyped_window_dictionary_value as CFTypeRef;
        if raw_window_dictionary_pointer.is_null() {
            continue;
        }

        let window_dictionary_cf_type =
            unsafe { CFType::wrap_under_get_rule(raw_window_dictionary_pointer) };
        let Some(untyped_window_dictionary) = window_dictionary_cf_type.downcast::<CFDictionary>()
        else {
            continue;
        };
        let typed_window_dictionary: CFDictionary<CFString, CFType> = unsafe {
            CFDictionary::wrap_under_get_rule(untyped_window_dictionary.as_concrete_TypeRef())
        };

        let owner_process_identifier = extract_window_dictionary_number_field(
            &typed_window_dictionary,
            &owner_process_identifier_key,
        );
        if owner_process_identifier != Some(frontmost_process_identifier) {
            continue;
        }

        let window_layer =
            extract_window_dictionary_number_field(&typed_window_dictionary, &window_layer_key);
        if window_layer != Some(0) {
            continue;
        }

        let window_title =
            extract_window_dictionary_string_field(&typed_window_dictionary, &window_title_key);
        if window_title.is_some() {
            return window_title;
        }
    }

    None
}

fn determine_focus_confidence_level(
    focused_window_is_present: bool,
    focused_browser_tab_is_present: bool,
    focused_browser_origin_is_present: bool,
) -> FocusConfidenceLevel {
    if focused_window_is_present && focused_browser_origin_is_present {
        FocusConfidenceLevel::High
    } else if focused_window_is_present || focused_browser_tab_is_present {
        FocusConfidenceLevel::Medium
    } else {
        FocusConfidenceLevel::Low
    }
}

fn collect_frontmost_application_metadata() -> Option<FrontmostApplicationMetadata> {
    let frontmost_application = get_frontmost_application()?;
    let display_name = frontmost_application
        .localizedName()
        .as_ref()
        .map_or_else(|| "Unknown".to_string(), nsstring_to_string);
    let bundle_identifier = frontmost_application
        .bundleIdentifier()
        .as_ref()
        .map(nsstring_to_string);
    let process_identifier = frontmost_application.processIdentifier();

    Some(FrontmostApplicationMetadata {
        display_name,
        bundle_identifier,
        process_identifier,
    })
}

fn build_focused_application(
    frontmost_application_metadata: Option<&FrontmostApplicationMetadata>,
) -> Option<FocusedApplication> {
    frontmost_application_metadata.map(|frontmost_application_metadata| FocusedApplication {
        display_name: frontmost_application_metadata.display_name.clone(),
        bundle_id: frontmost_application_metadata.bundle_identifier.clone(),
        process_path: None,
    })
}

fn get_accessibility_focused_window_details_for_frontmost_application(
    frontmost_application_metadata: Option<&FrontmostApplicationMetadata>,
) -> Option<AccessibilityFocusedWindowDetails> {
    if !is_accessibility_api_trusted() {
        return None;
    }

    let process_identifier = frontmost_application_metadata?.process_identifier;
    get_accessibility_focused_window_details(process_identifier)
}

fn determine_focused_window_title(
    frontmost_application_metadata: Option<&FrontmostApplicationMetadata>,
    accessibility_focused_window_details: Option<&AccessibilityFocusedWindowDetails>,
) -> Option<String> {
    accessibility_focused_window_details
        .and_then(|focused_window_details| focused_window_details.focused_window_title.clone())
        .or_else(|| {
            frontmost_application_metadata.and_then(|frontmost_application_metadata| {
                get_frontmost_window_title_from_core_graphics(
                    frontmost_application_metadata.process_identifier,
                )
            })
        })
}

fn build_focused_browser_tab(
    frontmost_application_metadata: Option<&FrontmostApplicationMetadata>,
    focused_window_title: Option<&str>,
    accessibility_focused_window_details: Option<&AccessibilityFocusedWindowDetails>,
) -> Option<FocusedBrowserTab> {
    let browser_name =
        frontmost_application_metadata.and_then(|frontmost_application_metadata| {
            frontmost_application_metadata
                .bundle_identifier
                .as_deref()
                .and_then(browser_name_from_bundle_identifier)
        })?;
    let normalized_browser_document_origin = accessibility_focused_window_details
        .and_then(|focused_window_details| focused_window_details.focused_document_url.as_deref())
        .and_then(normalize_browser_document_origin);
    let inferred_browser_tab_title =
        infer_browser_tab_title_from_window_title(focused_window_title, browser_name);
    if inferred_browser_tab_title.is_none() && normalized_browser_document_origin.is_none() {
        return None;
    }

    Some(FocusedBrowserTab {
        title: inferred_browser_tab_title,
        origin: normalized_browser_document_origin,
        browser: Some(browser_name.to_string()),
    })
}

pub fn get_current_focus_context() -> FocusContextSnapshot {
    let captured_at = chrono::Utc::now().to_rfc3339();
    let frontmost_application_metadata = collect_frontmost_application_metadata();
    let accessibility_focused_window_details =
        get_accessibility_focused_window_details_for_frontmost_application(
            frontmost_application_metadata.as_ref(),
        );
    let focused_application = build_focused_application(frontmost_application_metadata.as_ref());
    let focused_window_title = determine_focused_window_title(
        frontmost_application_metadata.as_ref(),
        accessibility_focused_window_details.as_ref(),
    );
    let focused_window = focused_window_title
        .as_ref()
        .map(|focused_window_title| FocusedWindow {
            title: focused_window_title.clone(),
        });
    let focused_browser_tab = build_focused_browser_tab(
        frontmost_application_metadata.as_ref(),
        focused_window_title.as_deref(),
        accessibility_focused_window_details.as_ref(),
    );
    let focused_browser_origin_is_present = focused_browser_tab
        .as_ref()
        .and_then(|focused_browser_tab| focused_browser_tab.origin.as_ref())
        .is_some();
    let event_source = if accessibility_focused_window_details.is_some() {
        FocusEventSource::Accessibility
    } else {
        FocusEventSource::Polling
    };
    let confidence_level = determine_focus_confidence_level(
        focused_window.is_some(),
        focused_browser_tab.is_some(),
        focused_browser_origin_is_present,
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
#[path = "../tests/focus_macos_tests.rs"]
mod focus_macos_tests;
