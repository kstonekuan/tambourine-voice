import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Store } from "@tauri-apps/plugin-store";
import ky from "ky";

// =============================================================================
// Provider ID Constants - Must match server's provider_registry.py
// =============================================================================

/** STT provider IDs from server/services/provider_registry.py */
export const STT_PROVIDER_IDS = [
	"auto",
	"speechmatics",
	"assemblyai",
	"aws",
	"azure",
	"cartesia",
	"deepgram",
	"google",
	"groq",
	"nemotron",
	"openai",
	"whisper",
] as const;

/** LLM provider IDs from server/services/provider_registry.py */
export const LLM_PROVIDER_IDS = [
	"auto",
	"anthropic",
	"cerebras",
	"gemini",
	"groq",
	"ollama",
	"openai",
	"openrouter",
] as const;

/** Known STT provider IDs that we have type definitions for */
export type KnownSTTProviderId = (typeof STT_PROVIDER_IDS)[number];

/** Known LLM provider IDs that we have type definitions for */
export type KnownLLMProviderId = (typeof LLM_PROVIDER_IDS)[number];

/**
 * Valid STT provider ID - includes known providers plus any string for forward compatibility.
 * When server adds new providers, old clients can still use them (as unknown strings).
 * The `(string & {})` trick preserves autocomplete for known values while accepting any string.
 */
export type STTProviderId = KnownSTTProviderId | (string & {});

/**
 * Valid LLM provider ID - includes known providers plus any string for forward compatibility.
 */
export type LLMProviderId = KnownLLMProviderId | (string & {});

// Type guards (isKnownSTTProvider, isKnownLLMProvider) can be added here if needed
// to detect unknown providers from newer servers and show visual indicators in the UI.

// Re-export event types for consumers that import from tauri.ts
export type { ConfigResponse, ConnectionState } from "./events";

import {
	AppEvents,
	type ConfigResponse,
	type ConnectionState,
	emitEvent,
	listenEvent,
} from "./events";

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

/**
 * Discriminated union for prompt section configuration.
 * - Auto mode: server uses built-in default prompt
 * - Manual mode: server uses user-provided content
 */
export type PromptSection =
	| { enabled: boolean; mode: "auto" }
	| { enabled: boolean; mode: "manual"; content: string };

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
	stt_provider: STTProviderId;
	llm_provider: LLMProviderId;
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
		return listenEvent(AppEvents.recordingStart, callback);
	},

	async onStopRecording(callback: () => void): Promise<UnlistenFn> {
		return listenEvent(AppEvents.recordingStop, callback);
	},

	async onPrepareRecording(callback: () => void): Promise<UnlistenFn> {
		return listenEvent(AppEvents.prepareRecording, callback);
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

	async updateSTTProvider(provider: STTProviderId): Promise<void> {
		return invoke("update_stt_provider", { provider });
	},

	async updateLLMProvider(provider: LLMProviderId): Promise<void> {
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
		return emitEvent(AppEvents.connectionState, { state });
	},

	async onConnectionStateChanged(
		callback: (state: ConnectionState) => void,
	): Promise<UnlistenFn> {
		return listenEvent(AppEvents.connectionState, (payload) => {
			callback(payload.state);
		});
	},

	// History sync between windows
	async emitHistoryChanged(): Promise<void> {
		return emitEvent(AppEvents.historyChanged);
	},

	async onHistoryChanged(callback: () => void): Promise<UnlistenFn> {
		return listenEvent(AppEvents.historyChanged, callback);
	},

	// Settings sync between windows (main -> overlay)
	async emitSettingsChanged(): Promise<void> {
		return emitEvent(AppEvents.settingsChanged);
	},

	async onSettingsChanged(callback: () => void): Promise<UnlistenFn> {
		return listenEvent(AppEvents.settingsChanged, callback);
	},

	// Reconnect request (main -> overlay)
	async emitReconnect(): Promise<void> {
		return emitEvent(AppEvents.reconnectRequest);
	},

	async onReconnect(callback: () => void): Promise<UnlistenFn> {
		return listenEvent(AppEvents.reconnectRequest, callback);
	},

	// Reconnection status (overlay -> main)
	async emitReconnectStarted(): Promise<void> {
		return emitEvent(AppEvents.reconnectStarted);
	},

	async onReconnectStarted(callback: () => void): Promise<UnlistenFn> {
		return listenEvent(AppEvents.reconnectStarted, callback);
	},

	async emitReconnectResult(success: boolean, error?: string): Promise<void> {
		return emitEvent(AppEvents.reconnectResult, { success, error });
	},

	async onReconnectResult(
		callback: (result: { success: boolean; error?: string }) => void,
	): Promise<UnlistenFn> {
		return listenEvent(AppEvents.reconnectResult, callback);
	},

	// Config response notifications (from Rust or overlay)
	async emitConfigResponse(response: ConfigResponse): Promise<void> {
		return emitEvent(AppEvents.configResponse, response);
	},

	async onConfigResponse(
		callback: (response: ConfigResponse) => void,
	): Promise<UnlistenFn> {
		return listenEvent(AppEvents.configResponse, callback);
	},

	// Server connection state management (for Rust-side config syncing)
	async setServerConnected(
		serverUrl: string,
		clientUuid: string,
	): Promise<void> {
		return invoke("set_server_connected", { serverUrl, clientUuid });
	},

	async setServerDisconnected(): Promise<void> {
		return invoke("set_server_disconnected");
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
	// =========================================================================
	// Static endpoints (no client UUID needed)
	// =========================================================================

	// Static prompt defaults
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

	// Get available providers (global, no UUID required)
	getAvailableProviders: async (
		serverUrl: string,
	): Promise<AvailableProvidersData> => {
		const api = createApiClient(serverUrl);
		return api.get("api/providers").json<AvailableProvidersData>();
	},
};
