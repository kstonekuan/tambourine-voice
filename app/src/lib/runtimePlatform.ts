import { type Platform, platform } from "@tauri-apps/plugin-os";

let cachedRuntimePlatform: Platform | "unknown" | null = null;

function detectRuntimePlatformWithFallback(): Platform | "unknown" {
	if (cachedRuntimePlatform) {
		return cachedRuntimePlatform;
	}

	if (
		typeof window === "undefined" ||
		window.__TAURI_OS_PLUGIN_INTERNALS__ === undefined
	) {
		cachedRuntimePlatform = "unknown";
		return cachedRuntimePlatform;
	}

	try {
		cachedRuntimePlatform = platform();
	} catch (runtimePlatformDetectionError) {
		console.warn(
			"[Platform] Failed to detect runtime platform, defaulting to non-macOS behavior:",
			runtimePlatformDetectionError,
		);
		cachedRuntimePlatform = "unknown";
	}

	return cachedRuntimePlatform;
}

export function isMacOSRuntime(): boolean {
	return detectRuntimePlatformWithFallback() === "macos";
}
