use crate::settings::{check_hotkey_conflict, AppSettings, HotkeyConfig, HotkeyType};

// Tests for HotkeyConfig::to_shortcut_string()
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
    let settings = AppSettings::default();
    let new_hotkey = HotkeyConfig {
        modifiers: vec!["ctrl".to_string(), "shift".to_string()],
        key: "A".to_string(),
        enabled: true,
    };
    assert!(check_hotkey_conflict(&new_hotkey, &settings, HotkeyType::Toggle).is_none());
}

#[test]
fn test_check_hotkey_conflict_allows_same_type() {
    let settings = AppSettings::default();
    // Using toggle's default hotkey when editing toggle should be allowed
    let new_hotkey = HotkeyConfig::default_toggle();
    assert!(check_hotkey_conflict(&new_hotkey, &settings, HotkeyType::Toggle).is_none());
}

#[test]
fn test_check_hotkey_conflict_detects_conflict_with_hold() {
    let settings = AppSettings::default();
    // Trying to use hold's hotkey for toggle should fail
    let new_hotkey = HotkeyConfig::default_hold();
    let result = check_hotkey_conflict(&new_hotkey, &settings, HotkeyType::Toggle);
    assert!(result.is_some());
}

#[test]
fn test_check_hotkey_conflict_detects_conflict_with_paste_last() {
    let settings = AppSettings::default();
    // Trying to use paste_last's hotkey for toggle should fail
    let new_hotkey = HotkeyConfig::default_paste_last();
    let result = check_hotkey_conflict(&new_hotkey, &settings, HotkeyType::Toggle);
    assert!(result.is_some());
}

// Tests for AppSettings::default()
#[test]
fn test_app_settings_default() {
    let settings = AppSettings::default();
    assert_eq!(settings.toggle_hotkey, HotkeyConfig::default_toggle());
    assert_eq!(settings.hold_hotkey, HotkeyConfig::default_hold());
    assert_eq!(
        settings.paste_last_hotkey,
        HotkeyConfig::default_paste_last()
    );
    assert!(settings.sound_enabled);
    assert!(!settings.auto_mute_audio);
    assert!(settings.selected_mic_id.is_none());
    assert!(settings.stt_provider.is_none());
    assert!(settings.llm_provider.is_none());
    assert!(settings.cleanup_prompt_sections.is_none());
    assert!(settings.stt_timeout_seconds.is_none());
    assert_eq!(settings.server_url, "http://127.0.0.1:8765");
}

// Tests for HotkeyType
#[test]
fn test_hotkey_type_store_key() {
    assert_eq!(HotkeyType::Toggle.store_key(), "toggle_hotkey");
    assert_eq!(HotkeyType::Hold.store_key(), "hold_hotkey");
    assert_eq!(HotkeyType::PasteLast.store_key(), "paste_last_hotkey");
}

#[test]
fn test_hotkey_type_display_name() {
    assert_eq!(HotkeyType::Toggle.display_name(), "toggle");
    assert_eq!(HotkeyType::Hold.display_name(), "hold");
    assert_eq!(HotkeyType::PasteLast.display_name(), "paste last");
}
