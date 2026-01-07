use crate::settings::HotkeyConfig;

// Tests for HotkeyConfig
#[test]
fn test_default_toggle_hotkey() {
    let hotkey = HotkeyConfig::default_toggle();
    assert_eq!(hotkey.key, "Space");
    assert!(hotkey.modifiers.contains(&"ctrl".to_string()));
    assert!(hotkey.modifiers.contains(&"alt".to_string()));
}

#[test]
fn test_default_hold_hotkey() {
    let hotkey = HotkeyConfig::default_hold();
    assert_eq!(hotkey.key, "Backquote");
    assert!(hotkey.modifiers.contains(&"ctrl".to_string()));
    assert!(hotkey.modifiers.contains(&"alt".to_string()));
}

#[test]
fn test_default_paste_last_hotkey() {
    let hotkey = HotkeyConfig::default_paste_last();
    assert_eq!(hotkey.key, "Period");
    assert!(hotkey.modifiers.contains(&"ctrl".to_string()));
    assert!(hotkey.modifiers.contains(&"alt".to_string()));
}

#[test]
fn test_to_shortcut_string() {
    let hotkey = HotkeyConfig {
        modifiers: vec!["Ctrl".to_string(), "Alt".to_string()],
        key: "Space".to_string(),
        enabled: true,
    };
    // Modifiers should be lowercased
    let result = hotkey.to_shortcut_string();
    assert!(result.contains("ctrl"));
    assert!(result.contains("alt"));
    assert!(result.contains("Space"));
}

#[test]
fn test_default_hotkeys_are_enabled() {
    assert!(HotkeyConfig::default_toggle().enabled);
    assert!(HotkeyConfig::default_hold().enabled);
    assert!(HotkeyConfig::default_paste_last().enabled);
}
