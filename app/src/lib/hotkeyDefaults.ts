import type { HotkeyConfig } from "./tauri";

// ============================================================================
// DEFAULT HOTKEY CONSTANTS - Single source of truth for all default hotkeys
// These must match the Rust defaults in settings.rs
// ============================================================================

/** Default modifiers for all hotkeys */
export const DEFAULT_HOTKEY_MODIFIERS = ["ctrl", "alt"];

/** Default key for toggle recording (Ctrl+Alt+Space) */
export const DEFAULT_TOGGLE_KEY = "Space";

/** Default key for hold-to-record (Ctrl+Alt+`) */
export const DEFAULT_HOLD_KEY = "Backquote";

/** Default key for paste last transcription (Ctrl+Alt+.) */
export const DEFAULT_PASTE_LAST_KEY = "Period";

// ============================================================================

/** Default toggle hotkey config */
export const DEFAULT_TOGGLE_HOTKEY: HotkeyConfig = {
	modifiers: DEFAULT_HOTKEY_MODIFIERS,
	key: DEFAULT_TOGGLE_KEY,
	enabled: true,
};

/** Default hold-to-record hotkey config */
export const DEFAULT_HOLD_HOTKEY: HotkeyConfig = {
	modifiers: DEFAULT_HOTKEY_MODIFIERS,
	key: DEFAULT_HOLD_KEY,
	enabled: true,
};

/** Default paste last transcription hotkey config */
export const DEFAULT_PASTE_LAST_HOTKEY: HotkeyConfig = {
	modifiers: DEFAULT_HOTKEY_MODIFIERS,
	key: DEFAULT_PASTE_LAST_KEY,
	enabled: true,
};
