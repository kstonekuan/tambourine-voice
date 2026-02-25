import { isMacOSRuntime } from "./runtimePlatform";

const MAC_MODIFIER_SYMBOLS: Record<string, string> = {
	ctrl: "⌃",
	alt: "⌥",
	shift: "⇧",
	meta: "⌘",
	mod: "⌘",
};

function capitalize(key: string): string {
	return key.charAt(0).toUpperCase() + key.slice(1);
}

/**
 * Format a key name for display based on the current platform.
 * On macOS, modifier keys are shown as symbols (⌃, ⌥, ⇧, ⌘).
 * On other platforms, keys are simply capitalized.
 */
export function formatKeyForPlatform(key: string): string {
	if (isMacOSRuntime()) {
		const symbol = MAC_MODIFIER_SYMBOLS[key.toLowerCase()];
		if (symbol) {
			return symbol;
		}
	}
	return capitalize(key);
}
