use crate::settings::HotkeyConfig;

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
