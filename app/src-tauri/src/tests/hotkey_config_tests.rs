use crate::settings::{
    check_hotkey_conflict, AppSettings, HotkeyConfig, HotkeyType, LocalOnlySetting, SettingsError,
};

// Tests for HotkeyConfig::to_shortcut_string()
fn make_hotkey(modifiers: &[&str], key: &str) -> HotkeyConfig {
    HotkeyConfig {
        modifiers: modifiers
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        key: key.to_string(),
        enabled: true,
    }
}

fn make_settings_with_hotkeys(
    toggle_hotkey: HotkeyConfig,
    hold_hotkey: HotkeyConfig,
    paste_last_hotkey: HotkeyConfig,
) -> AppSettings {
    AppSettings {
        toggle_hotkey,
        hold_hotkey,
        paste_last_hotkey,
        ..AppSettings::default()
    }
}

#[test]
fn test_to_shortcut_string_single_modifier() {
    let hotkey = HotkeyConfig {
        key: "Space".to_string(),
        modifiers: vec!["Ctrl".to_string()],
        enabled: true,
    };
    assert_eq!(hotkey.to_shortcut_string(), "ctrl+Space");
}

#[test]
fn test_to_shortcut_string_multiple_modifiers() {
    let hotkey = HotkeyConfig {
        key: "Space".to_string(),
        modifiers: vec!["Ctrl".to_string(), "Alt".to_string()],
        enabled: true,
    };
    assert_eq!(hotkey.to_shortcut_string(), "ctrl+alt+Space");
}

#[test]
fn test_to_shortcut_string_preserves_key_case() {
    let hotkey = HotkeyConfig {
        key: "Backquote".to_string(),
        modifiers: vec!["CTRL".to_string(), "ALT".to_string()],
        enabled: true,
    };
    // Modifiers should be lowercase, key should preserve case
    assert_eq!(hotkey.to_shortcut_string(), "ctrl+alt+Backquote");
}

// Tests for HotkeyConfig::is_same_as()
#[test]
fn test_is_same_as_identical_hotkeys() {
    let a = HotkeyConfig {
        modifiers: vec!["ctrl".to_string(), "alt".to_string()],
        key: "Space".to_string(),
        enabled: true,
    };
    let b = HotkeyConfig {
        modifiers: vec!["ctrl".to_string(), "alt".to_string()],
        key: "Space".to_string(),
        enabled: true,
    };
    assert!(a.is_same_as(&b));
}

#[test]
fn test_is_same_as_case_insensitive_keys() {
    let a = HotkeyConfig {
        modifiers: vec!["ctrl".to_string()],
        key: "space".to_string(),
        enabled: true,
    };
    let b = HotkeyConfig {
        modifiers: vec!["ctrl".to_string()],
        key: "SPACE".to_string(),
        enabled: true,
    };
    assert!(a.is_same_as(&b));
}

#[test]
fn test_is_same_as_case_insensitive_modifiers() {
    let a = HotkeyConfig {
        modifiers: vec!["CTRL".to_string(), "ALT".to_string()],
        key: "Space".to_string(),
        enabled: true,
    };
    let b = HotkeyConfig {
        modifiers: vec!["ctrl".to_string(), "alt".to_string()],
        key: "Space".to_string(),
        enabled: true,
    };
    assert!(a.is_same_as(&b));
}

#[test]
fn test_is_same_as_modifiers_different_order() {
    let a = HotkeyConfig {
        modifiers: vec!["ctrl".to_string(), "alt".to_string()],
        key: "Space".to_string(),
        enabled: true,
    };
    let b = HotkeyConfig {
        modifiers: vec!["alt".to_string(), "ctrl".to_string()],
        key: "Space".to_string(),
        enabled: true,
    };
    assert!(a.is_same_as(&b));
}

#[test]
fn test_is_same_as_different_keys() {
    let a = HotkeyConfig {
        modifiers: vec!["ctrl".to_string()],
        key: "Space".to_string(),
        enabled: true,
    };
    let b = HotkeyConfig {
        modifiers: vec!["ctrl".to_string()],
        key: "Enter".to_string(),
        enabled: true,
    };
    assert!(!a.is_same_as(&b));
}

#[test]
fn test_is_same_as_different_modifiers() {
    let a = HotkeyConfig {
        modifiers: vec!["ctrl".to_string()],
        key: "Space".to_string(),
        enabled: true,
    };
    let b = HotkeyConfig {
        modifiers: vec!["alt".to_string()],
        key: "Space".to_string(),
        enabled: true,
    };
    assert!(!a.is_same_as(&b));
}

#[test]
fn test_is_same_as_different_modifier_counts() {
    let a = HotkeyConfig {
        modifiers: vec!["ctrl".to_string(), "alt".to_string()],
        key: "Space".to_string(),
        enabled: true,
    };
    let b = HotkeyConfig {
        modifiers: vec!["ctrl".to_string()],
        key: "Space".to_string(),
        enabled: true,
    };
    assert!(!a.is_same_as(&b));
}

// Tests for check_hotkey_conflict()
#[test]
fn test_check_hotkey_conflict_no_conflict() {
    let settings = make_settings_with_hotkeys(
        make_hotkey(&["ctrl", "alt"], "Space"),
        make_hotkey(&["ctrl", "alt"], "Backquote"),
        make_hotkey(&["ctrl", "alt"], "Period"),
    );
    let new_hotkey = make_hotkey(&["ctrl", "shift"], "A");
    assert!(check_hotkey_conflict(&new_hotkey, &settings, HotkeyType::Toggle).is_none());
}

#[test]
fn test_check_hotkey_conflict_allows_same_type() {
    let toggle_hotkey = make_hotkey(&["ctrl", "alt"], "Space");
    let settings = make_settings_with_hotkeys(
        toggle_hotkey.clone(),
        make_hotkey(&["ctrl", "alt"], "Backquote"),
        make_hotkey(&["ctrl", "alt"], "Period"),
    );
    // Reusing the current toggle hotkey while editing toggle is valid.
    let new_hotkey = toggle_hotkey;
    assert!(check_hotkey_conflict(&new_hotkey, &settings, HotkeyType::Toggle).is_none());
}

#[test]
fn test_check_hotkey_conflict_detects_conflict_with_hold_and_reports_type() {
    let hold_hotkey = make_hotkey(&["ctrl", "alt"], "Backquote");
    let settings = make_settings_with_hotkeys(
        make_hotkey(&["ctrl", "alt"], "Space"),
        hold_hotkey.clone(),
        make_hotkey(&["ctrl", "alt"], "Period"),
    );
    // Trying to use hold's hotkey for toggle should fail.
    let new_hotkey = hold_hotkey;
    let result = check_hotkey_conflict(&new_hotkey, &settings, HotkeyType::Toggle);
    assert!(matches!(
        result,
        Some(SettingsError::HotkeyConflict {
            conflicting_type: HotkeyType::Hold,
            ..
        })
    ));
}

#[test]
fn test_check_hotkey_conflict_detects_conflict_with_paste_last_and_reports_type() {
    let paste_last_hotkey = make_hotkey(&["ctrl", "alt"], "Period");
    let settings = make_settings_with_hotkeys(
        make_hotkey(&["ctrl", "alt"], "Space"),
        make_hotkey(&["ctrl", "alt"], "Backquote"),
        paste_last_hotkey.clone(),
    );
    // Trying to use paste-last's hotkey for toggle should fail.
    let new_hotkey = paste_last_hotkey;
    let result = check_hotkey_conflict(&new_hotkey, &settings, HotkeyType::Toggle);
    assert!(matches!(
        result,
        Some(SettingsError::HotkeyConflict {
            conflicting_type: HotkeyType::PasteLast,
            ..
        })
    ));
}

// Tests for HotkeyType
#[test]
fn test_hotkey_type_local_only_setting() {
    assert_eq!(
        HotkeyType::Toggle.local_only_setting(),
        LocalOnlySetting::ToggleHotkey
    );
    assert_eq!(
        HotkeyType::Hold.local_only_setting(),
        LocalOnlySetting::HoldHotkey
    );
    assert_eq!(
        HotkeyType::PasteLast.local_only_setting(),
        LocalOnlySetting::PasteLastHotkey
    );
}

#[test]
fn test_hotkey_type_display_name() {
    assert_eq!(HotkeyType::Toggle.display_name(), "toggle");
    assert_eq!(HotkeyType::Hold.display_name(), "hold");
    assert_eq!(HotkeyType::PasteLast.display_name(), "paste last");
}
