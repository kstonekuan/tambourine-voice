import { invoke } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Store } from "@tauri-apps/plugin-store";
import ky from "ky";

export type ConnectionState =
	| "disconnected"
	| "connecting"
	| "reconnecting"
	| "idle"
	| "recording"
	| "processing";

/**
 * Discriminated union for config responses.
 * Each variant has only its relevant fields, enabling exhaustive pattern matching.
 */
export type ConfigResponse =
	| { type: "config-updated"; setting: string; value: unknown }
	| { type: "config-error"; setting: string; error: string };

interface TypeTextResult {
	success: boolean;
	error?: string;
}

export interface HotkeyConfig {
	modifiers: string[];
	key: string;
	enabled: boolean;
}

/// Tracks errors from shortcut registration attempts
export interface ShortcutErrors {
	toggle_error: string | null;
	hold_error: string | null;
	paste_last_error: string | null;
}

/// Result of shortcut registration attempt
export interface ShortcutRegistrationResult {
	toggle_registered: boolean;
	hold_registered: boolean;
	paste_last_registered: boolean;
	errors: ShortcutErrors;
}

interface HistoryEntry {
	id: string;
	timestamp: string;
	text: string;
}

export interface PromptSection {
	enabled: boolean;
	content: string | null;
}

export interface CleanupPromptSections {
	main: PromptSection;
	advanced: PromptSection;
	dictionary: PromptSection;
}

export interface AppSettings {
	toggle_hotkey: HotkeyConfig;
	hold_hotkey: HotkeyConfig;
	paste_last_hotkey: HotkeyConfig;
	selected_mic_id: string | null;
	sound_enabled: boolean;
	cleanup_prompt_sections: CleanupPromptSections | null;
	stt_provider: string | null;
	llm_provider: string | null;
	auto_mute_audio: boolean;
	stt_timeout_seconds: number | null;
	server_url: string;
}

export const DEFAULT_SERVER_URL = "http://127.0.0.1:8765";

let storeInstance: Store | null = null;

async function getStore(): Promise<Store> {
	if (!storeInstance) {
		storeInstance = await Store.load("settings.json");
	}
	return storeInstance;
}

// ============================================================================
// Hotkey validation helpers (for immediate UI feedback)
// Rust provides the same validation as a safety net on save
// ============================================================================

/**
 * Check if two hotkey configs are equivalent (case-insensitive comparison)
 */
export function hotkeyIsSameAs(a: HotkeyConfig, b: HotkeyConfig): boolean {
	if (a.key.toLowerCase() !== b.key.toLowerCase()) return false;
	if (a.modifiers.length !== b.modifiers.length) return false;
	return a.modifiers.every((mod) =>
		b.modifiers.some((other) => mod.toLowerCase() === other.toLowerCase()),
	);
}

export type HotkeyType = "toggle" | "hold" | "paste_last";

const HOTKEY_LABELS: Record<HotkeyType, string> = {
	toggle: "toggle",
	hold: "hold",
	paste_last: "paste last",
};

/**
 * Validate that a hotkey doesn't conflict with other hotkeys
 * Returns error message if invalid, null if valid
 * Used for immediate UI feedback - Rust provides the same validation as a safety net
 */
export function validateHotkeyNotDuplicate(
	newHotkey: HotkeyConfig,
	allHotkeys: {
		toggle: HotkeyConfig;
		hold: HotkeyConfig;
		paste_last: HotkeyConfig;
	},
	excludeType: HotkeyType,
): string | null {
	for (const [type, existing] of Object.entries(allHotkeys)) {
		if (type !== excludeType && hotkeyIsSameAs(newHotkey, existing)) {
			return `This shortcut is already used for the ${HOTKEY_LABELS[type as HotkeyType]} hotkey`;
		}
	}
	return null;
}

export const tauriAPI = {
	async typeText(text: string): Promise<TypeTextResult> {
		try {
			await invoke("type_text", { text });
			return { success: true };
		} catch (error) {
			return { success: false, error: String(error) };
		}
	},

	async getServerUrl(): Promise<string> {
		return invoke("get_server_url");
	},

	// Client UUID management for server identification
	async getClientUUID(): Promise<string | null> {
		const store = await getStore();
		return (await store.get<string | null>("client_uuid")) ?? null;
	},

	async setClientUUID(uuid: string): Promise<void> {
		const store = await getStore();
		await store.set("client_uuid", uuid);
		await store.save();
	},

	async clearClientUUID(): Promise<void> {
		const store = await getStore();
		await store.delete("client_uuid");
		await store.save();
	},

	async onStartRecording(callback: () => void): Promise<UnlistenFn> {
		return listen("recording-start", callback);
	},

	async onStopRecording(callback: () => void): Promise<UnlistenFn> {
		return listen("recording-stop", callback);
	},

	async onPrepareRecording(callback: () => void): Promise<UnlistenFn> {
		return listen("prepare-recording", callback);
	},

	// Settings API - uses Rust commands for single source of truth
	// Rust applies defaults, TypeScript just passes through
	async getSettings(): Promise<AppSettings> {
		return invoke("get_settings");
	},

	async updateToggleHotkey(hotkey: HotkeyConfig): Promise<void> {
		return invoke("update_hotkey", { hotkeyType: "toggle", config: hotkey });
	},

	async updateHoldHotkey(hotkey: HotkeyConfig): Promise<void> {
		return invoke("update_hotkey", { hotkeyType: "hold", config: hotkey });
	},

	async updatePasteLastHotkey(hotkey: HotkeyConfig): Promise<void> {
		return invoke("update_hotkey", {
			hotkeyType: "paste_last",
			config: hotkey,
		});
	},

	async updateSelectedMic(micId: string | null): Promise<void> {
		return invoke("update_selected_mic", { micId });
	},

	async updateSoundEnabled(enabled: boolean): Promise<void> {
		return invoke("update_sound_enabled", { enabled });
	},

	async updateCleanupPromptSections(
		sections: CleanupPromptSections | null,
	): Promise<void> {
		return invoke("update_cleanup_prompt_sections", { sections });
	},

	async updateSTTProvider(provider: string | null): Promise<void> {
		return invoke("update_stt_provider", { provider });
	},

	async updateLLMProvider(provider: string | null): Promise<void> {
		return invoke("update_llm_provider", { provider });
	},

	async updateAutoMuteAudio(enabled: boolean): Promise<void> {
		return invoke("update_auto_mute_audio", { enabled });
	},

	async updateSTTTimeout(timeoutSeconds: number | null): Promise<void> {
		return invoke("update_stt_timeout", { timeoutSeconds });
	},

	async updateServerUrl(url: string): Promise<void> {
		return invoke("update_server_url", { url });
	},

	async isAudioMuteSupported(): Promise<boolean> {
		return invoke("is_audio_mute_supported");
	},

	async resetHotkeysToDefaults(): Promise<void> {
		return invoke("reset_hotkeys_to_defaults");
	},

	async registerShortcuts(): Promise<ShortcutRegistrationResult> {
		return invoke("register_shortcuts");
	},

	async unregisterShortcuts(): Promise<void> {
		return invoke("unregister_shortcuts");
	},

	async getShortcutErrors(): Promise<ShortcutErrors> {
		return invoke("get_shortcut_errors");
	},

	async setHotkeyEnabled(
		hotkeyType: "toggle" | "hold" | "paste_last",
		enabled: boolean,
	): Promise<void> {
		return invoke("set_hotkey_enabled", { hotkeyType, enabled });
	},

	// History API
	async addHistoryEntry(text: string): Promise<HistoryEntry> {
		return invoke("add_history_entry", { text });
	},

	async getHistory(limit?: number): Promise<HistoryEntry[]> {
		return invoke("get_history", { limit });
	},

	async deleteHistoryEntry(id: string): Promise<boolean> {
		return invoke("delete_history_entry", { id });
	},

	async clearHistory(): Promise<void> {
		return invoke("clear_history");
	},

	// Overlay API
	async resizeOverlay(width: number, height: number): Promise<void> {
		return invoke("resize_overlay", { width, height });
	},

	async startDragging(): Promise<void> {
		const window = getCurrentWindow();
		return window.startDragging();
	},

	// Connection state sync between windows
	async emitConnectionState(state: ConnectionState): Promise<void> {
		return emit("connection-state-changed", { state });
	},

	async onConnectionStateChanged(
		callback: (state: ConnectionState) => void,
	): Promise<UnlistenFn> {
		return listen<{ state: ConnectionState }>(
			"connection-state-changed",
			(event) => {
				callback(event.payload.state);
			},
		);
	},

	// History sync between windows
	async emitHistoryChanged(): Promise<void> {
		return emit("history-changed", {});
	},

	async onHistoryChanged(callback: () => void): Promise<UnlistenFn> {
		return listen("history-changed", () => {
			callback();
		});
	},

	// Settings sync between windows (main -> overlay)
	async emitSettingsChanged(): Promise<void> {
		return emit("settings-changed", {});
	},

	async onSettingsChanged(callback: () => void): Promise<UnlistenFn> {
		return listen("settings-changed", () => {
			callback();
		});
	},

	// Reconnect request (main -> overlay)
	async emitReconnect(): Promise<void> {
		return emit("request-reconnect", {});
	},

	async onReconnect(callback: () => void): Promise<UnlistenFn> {
		return listen("request-reconnect", () => {
			callback();
		});
	},

	// Reconnection status (overlay -> main)
	async emitReconnectStarted(): Promise<void> {
		return emit("reconnect-started", {});
	},

	async onReconnectStarted(callback: () => void): Promise<UnlistenFn> {
		return listen("reconnect-started", () => {
			callback();
		});
	},

	async emitReconnectResult(success: boolean, error?: string): Promise<void> {
		return emit("reconnect-result", { success, error });
	},

	async onReconnectResult(
		callback: (result: { success: boolean; error?: string }) => void,
	): Promise<UnlistenFn> {
		return listen<{ success: boolean; error?: string }>(
			"reconnect-result",
			(event) => {
				callback(event.payload);
			},
		);
	},

	// Config response sync between windows (overlay -> main)
	async emitConfigResponse(response: ConfigResponse): Promise<void> {
		return emit("config-response", response);
	},

	async onConfigResponse(
		callback: (response: ConfigResponse) => void,
	): Promise<UnlistenFn> {
		return listen<ConfigResponse>("config-response", (event) => {
			callback(event.payload);
		});
	},

	// Available providers sync between windows (overlay -> main)
	async emitAvailableProviders(data: AvailableProvidersData): Promise<void> {
		return emit("available-providers", data);
	},

	async onAvailableProviders(
		callback: (data: AvailableProvidersData) => void,
	): Promise<UnlistenFn> {
		return listen<AvailableProvidersData>("available-providers", (event) => {
			callback(event.payload);
		});
	},
};

export interface DefaultSectionsResponse {
	main: string;
	advanced: string;
	dictionary: string;
}

export interface ProviderInfo {
	value: string;
	label: string;
	is_local: boolean;
	model?: string | null;
}

export interface AvailableProvidersData {
	stt: ProviderInfo[];
	llm: ProviderInfo[];
}

// Create ky instance with sensible defaults for API calls
function createApiClient(serverUrl: string) {
	return ky.create({
		prefixUrl: serverUrl,
		timeout: 10000,
		retry: {
			limit: 2,
			methods: ["get", "post"],
		},
	});
}

export const configAPI = {
	// Static prompt defaults (runtime config goes via data channel)
	getDefaultSections: async (serverUrl: string) => {
		const api = createApiClient(serverUrl);
		return api
			.get("api/prompt/sections/default")
			.json<DefaultSectionsResponse>();
	},

	// Client registration for UUID-based identification
	registerClient: async (serverUrl: string): Promise<string> => {
		const api = createApiClient(serverUrl);
		const response = await api
			.post("api/client/register")
			.json<{ uuid: string }>();
		return response.uuid;
	},

	// Verify if a client UUID is still registered with the server
	verifyClient: async (
		serverUrl: string,
		clientUUID: string,
	): Promise<boolean> => {
		const api = createApiClient(serverUrl);
		const response = await api
			.get(`api/client/verify/${clientUUID}`)
			.json<{ registered: boolean }>();
		return response.registered;
	},
};
