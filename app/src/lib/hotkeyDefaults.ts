import type { HotkeyConfig } from "./tauri";

// These must match the Rust defaults in settings.rs
export const DEFAULT_HOTKEY_MODIFIERS = ["ctrl", "alt"];
export const DEFAULT_TOGGLE_KEY = "Space";
export const DEFAULT_HOLD_KEY = "Backquote";
export const DEFAULT_PASTE_LAST_KEY = "Period";

export const DEFAULT_TOGGLE_HOTKEY: HotkeyConfig = {
	modifiers: DEFAULT_HOTKEY_MODIFIERS,
	key: DEFAULT_TOGGLE_KEY,
	enabled: true,
};

export const DEFAULT_HOLD_HOTKEY: HotkeyConfig = {
	modifiers: DEFAULT_HOTKEY_MODIFIERS,
	key: DEFAULT_HOLD_KEY,
	enabled: true,
};

export const DEFAULT_PASTE_LAST_HOTKEY: HotkeyConfig = {
	modifiers: DEFAULT_HOTKEY_MODIFIERS,
	key: DEFAULT_PASTE_LAST_KEY,
	enabled: true,
};
