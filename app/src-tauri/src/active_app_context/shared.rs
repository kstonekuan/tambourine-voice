use crate::active_app_context::FocusConfidenceLevel;
use std::net::IpAddr;

pub(crate) fn normalize_non_empty_focus_text(raw_focus_text: &str) -> Option<String> {
    let trimmed_focus_text = raw_focus_text.trim();
    if trimmed_focus_text.is_empty() {
        None
    } else {
        Some(trimmed_focus_text.to_string())
    }
}

fn normalize_origin_from_absolute_url(trimmed_document_url: &str) -> Option<String> {
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

fn normalize_origin_from_scheme_less_url(trimmed_document_url: &str) -> Option<String> {
    if trimmed_document_url.contains(char::is_whitespace) {
        return None;
    }

    let authority_end_index = trimmed_document_url
        .find(['/', '?', '#'])
        .unwrap_or(trimmed_document_url.len());
    let authority_component = &trimmed_document_url[..authority_end_index];
    if authority_component.is_empty() {
        return None;
    }

    let authority_without_port = authority_component
        .split(':')
        .next()
        .unwrap_or(authority_component);
    let looks_like_domain_name = authority_without_port.contains('.');
    let looks_like_localhost = authority_without_port.eq_ignore_ascii_case("localhost");
    let looks_like_ip_address = authority_without_port.parse::<IpAddr>().is_ok();

    if !(looks_like_domain_name || looks_like_localhost || looks_like_ip_address) {
        return None;
    }

    Some(format!("https://{authority_component}"))
}

pub(crate) fn normalize_browser_document_origin(raw_document_url: &str) -> Option<String> {
    let trimmed_document_url = normalize_non_empty_focus_text(raw_document_url)?;
    normalize_origin_from_absolute_url(&trimmed_document_url)
        .or_else(|| normalize_origin_from_scheme_less_url(&trimmed_document_url))
}

pub(crate) fn infer_browser_tab_title_from_window_title(
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

pub(crate) fn determine_focus_confidence_level(
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
