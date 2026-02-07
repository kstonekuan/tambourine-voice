use crate::focus::FocusConfidenceLevel;

pub(crate) fn normalize_non_empty_focus_text(raw_focus_text: &str) -> Option<String> {
    let trimmed_focus_text = raw_focus_text.trim();
    if trimmed_focus_text.is_empty() {
        None
    } else {
        Some(trimmed_focus_text.to_string())
    }
}

pub(crate) fn normalize_browser_document_origin(raw_document_url: &str) -> Option<String> {
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
