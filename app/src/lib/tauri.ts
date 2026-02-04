import { invoke } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Store } from "@tauri-apps/plugin-store";
import ky from "ky";
import { withoutTrailingSlash } from "ufo";
import { z } from "zod";

// =============================================================================
// Provider ID Constants - Single source of truth
// =============================================================================

/** Known STT provider IDs (excluding "auto") */
export const STT_KNOWN_PROVIDER_IDS = [
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

/** All STT provider IDs including "auto" */
export const STT_PROVIDER_IDS = ["auto", ...STT_KNOWN_PROVIDER_IDS] as const;

/** Known LLM provider IDs (excluding "auto") */
export const LLM_KNOWN_PROVIDER_IDS = [
	"anthropic",
	"cerebras",
	"gemini",
	"groq",
	"ollama",
	"openai",
	"openrouter",
] as const;

/** All LLM provider IDs including "auto" */
export const LLM_PROVIDER_IDS = ["auto", ...LLM_KNOWN_PROVIDER_IDS] as const;

// Types derived from constants
/** Known STT provider IDs that the client recognizes */
export type KnownSTTProviderId = (typeof STT_KNOWN_PROVIDER_IDS)[number];

/** Known LLM provider IDs that the client recognizes */
export type KnownLLMProviderId = (typeof LLM_KNOWN_PROVIDER_IDS)[number];

/**
 * All STT provider IDs including "auto".
 * The `(string & {})` trick preserves autocomplete for known values while accepting any string.
 */
export type STTProviderId = (typeof STT_PROVIDER_IDS)[number] | (string & {});

/**
 * All LLM provider IDs including "auto".
 * The `(string & {})` trick preserves autocomplete for known values while accepting any string.
 */
export type LLMProviderId = (typeof LLM_PROVIDER_IDS)[number] | (string & {});

// =============================================================================
// Provider Mode Constants
// =============================================================================

/** Discriminator values for provider selection modes */
export const ProviderMode = {
	/** Use server's configured default provider */
	Auto: "auto",
	/** Known provider from type-safe list */
	Known: "known",
	/** Unknown provider (forward compatibility) */
	Other: "other",
} as const;

// =============================================================================
// Provider Selection Discriminated Unions
// =============================================================================

/** Auto mode: use server's configured default provider */
type AutoProviderSelection = { mode: typeof ProviderMode.Auto };

/** Known STT provider from the type-safe list */
type KnownSTTProviderSelection = {
	mode: typeof ProviderMode.Known;
	providerId: KnownSTTProviderId;
};

/** Unknown STT provider (forward compatibility) */
type OtherSTTProviderSelection = {
	mode: typeof ProviderMode.Other;
	providerId: string;
};

/** Known LLM provider from the type-safe list */
type KnownLLMProviderSelection = {
	mode: typeof ProviderMode.Known;
	providerId: KnownLLMProviderId;
};

/** Unknown LLM provider (forward compatibility) */
type OtherLLMProviderSelection = {
	mode: typeof ProviderMode.Other;
	providerId: string;
};

/** Discriminated union for STT provider selection */
export type STTProviderSelection =
	| AutoProviderSelection
	| KnownSTTProviderSelection
	| OtherSTTProviderSelection;

/** Discriminated union for LLM provider selection */
export type LLMProviderSelection =
	| AutoProviderSelection
	| KnownLLMProviderSelection
	| OtherLLMProviderSelection;

/**
 * Internal helper for converting provider IDs to selection objects.
 * Handles "auto", known providers, and unknown providers (forward compatibility).
 */
function toProviderSelectionInternal<KnownId extends string>(
	id: string,
	knownIds: readonly KnownId[],
):
	| { mode: typeof ProviderMode.Auto }
	| { mode: typeof ProviderMode.Known; providerId: KnownId }
	| { mode: typeof ProviderMode.Other; providerId: string } {
	if (id === ProviderMode.Auto) {
		return { mode: ProviderMode.Auto };
	}
	if ((knownIds as readonly string[]).includes(id)) {
		return { mode: ProviderMode.Known, providerId: id as KnownId };
	}
	return { mode: ProviderMode.Other, providerId: id };
}

/**
 * Convert a stored STT provider ID to a selection object for RTVI messages.
 * Handles "auto", known providers, and unknown providers (forward compatibility).
 */
export function toSTTProviderSelection(
	id: STTProviderId,
): STTProviderSelection {
	return toProviderSelectionInternal(id, STT_KNOWN_PROVIDER_IDS);
}

/**
 * Convert a stored LLM provider ID to a selection object for RTVI messages.
 * Handles "auto", known providers, and unknown providers (forward compatibility).
 */
export function toLLMProviderSelection(
	id: LLMProviderId,
): LLMProviderSelection {
	return toProviderSelectionInternal(id, LLM_KNOWN_PROVIDER_IDS);
}

// =============================================================================
// Zod Schemas for Server Response Parsing
// =============================================================================

/**
 * Primary schema for STT provider selection - validates "auto" or "known" with strict enum.
 * This is what we EXPECT from the server for providers we recognize.
 */
const STTProviderSelectionPrimarySchema = z.discriminatedUnion("mode", [
	z.object({ mode: z.literal(ProviderMode.Auto) }),
	z.object({
		mode: z.literal(ProviderMode.Known),
		providerId: z.enum(STT_KNOWN_PROVIDER_IDS),
	}),
]);

/**
 * Primary schema for LLM provider selection - validates "auto" or "known" with strict enum.
 * This is what we EXPECT from the server for providers we recognize.
 */
const LLMProviderSelectionPrimarySchema = z.discriminatedUnion("mode", [
	z.object({ mode: z.literal(ProviderMode.Auto) }),
	z.object({
		mode: z.literal(ProviderMode.Known),
		providerId: z.enum(LLM_KNOWN_PROVIDER_IDS),
	}),
]);

/**
 * Fallback schema - extracts providerId when primary enum validation fails.
 * Accepts "known" mode (for provider IDs the server recognizes but client doesn't)
 * and "other" mode. This enables forward compatibility: old clients can use
 * providers added in newer server versions.
 */
const ProviderIdFallbackSchema = z.object({
	mode: z.literal(ProviderMode.Known).or(z.literal(ProviderMode.Other)),
	providerId: z.string(),
});

/**
 * Parse STT provider selection from server response.
 * Mirrors server's parse_stt_provider_selection pattern:
 * 1. Try "auto" or "known" with validated enum
 * 2. If enum validation fails → fallback to "other"
 *
 * This enables forward compatibility: old clients can use new server providers.
 */
export function parseSTTProviderSelection(
	value: unknown,
): STTProviderSelection | null {
	// 1. Try primary schema (auto or known with validated enum)
	const primaryResult = STTProviderSelectionPrimarySchema.safeParse(value);
	if (primaryResult.success) {
		return primaryResult.data;
	}

	// 2. Fallback: server sent unknown provider → put in "other"
	const fallbackResult = ProviderIdFallbackSchema.safeParse(value);
	if (fallbackResult.success) {
		return {
			mode: ProviderMode.Other,
			providerId: fallbackResult.data.providerId,
		};
	}

	// 3. Complete structural failure
	console.warn("Failed to parse STT provider selection:", value);
	return null;
}

/**
 * Parse LLM provider selection from server response.
 * Mirrors server's parse_llm_provider_selection pattern:
 * 1. Try "auto" or "known" with validated enum
 * 2. If enum validation fails → fallback to "other"
 *
 * This enables forward compatibility: old clients can use new server providers.
 */
export function parseLLMProviderSelection(
	value: unknown,
): LLMProviderSelection | null {
	// 1. Try primary schema (auto or known with validated enum)
	const primaryResult = LLMProviderSelectionPrimarySchema.safeParse(value);
	if (primaryResult.success) {
		return primaryResult.data;
	}

	// 2. Fallback: server sent unknown provider → put in "other"
	const fallbackResult = ProviderIdFallbackSchema.safeParse(value);
	if (fallbackResult.success) {
		return {
			mode: ProviderMode.Other,
			providerId: fallbackResult.data.providerId,
		};
	}

	// 3. Complete structural failure
	console.warn("Failed to parse LLM provider selection:", value);
	return null;
}

/**
 * Extract provider ID string from a selection.
 * Use this when you just need the ID for storage/display.
 */
export function getProviderIdFromSelection(
	selection: STTProviderSelection | LLMProviderSelection,
): string {
	if (selection.mode === ProviderMode.Auto) {
		return ProviderMode.Auto;
	}
	return selection.providerId;
}

// =============================================================================
// Setting Names (Forward-Compatible)
// =============================================================================

/**
 * Known setting names matching server's SettingName enum.
 * Used for type-safe handling of config responses.
 */
export const KNOWN_SETTINGS = [
	"stt-provider",
	"llm-provider",
	"prompt-sections",
	"stt-timeout",
] as const;

export type {
	ConfigResponse,
	ConnectionState,
	LLMErrorPayload,
	ProviderChangeRequestPayload,
} from "./events";

import {
	AppEvents,
	type ConfigResponse,
	type ConnectionState,
	emitEvent,
	type LLMErrorPayload,
	listenEvent,
	type ProviderChangeRequestPayload,
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

export interface ShortcutErrors {
	toggle_error: string | null;
	hold_error: string | null;
	paste_last_error: string | null;
}

export interface ShortcutRegistrationResult {
	toggle_registered: boolean;
	hold_registered: boolean;
	paste_last_registered: boolean;
	errors: ShortcutErrors;
}

export interface HistoryEntry {
	id: string;
	timestamp: string;
	text: string;
	raw_text: string;
}

// =============================================================================
// Export/Import Types
// =============================================================================

/** Strategy for importing history entries */
export type HistoryImportStrategy =
	| "replace"
	| "merge_append"
	| "merge_deduplicate";

/** Result of a history import operation */
export interface HistoryImportResult {
	success: boolean;
	entries_imported: number | null;
	entries_skipped: number | null;
}

/** Detected file type from import */
export type DetectedFileType = "settings" | "history" | "unknown";

/** Prompt section names */
export type PromptSectionName = "main" | "advanced" | "dictionary";

/**
 * Mode of prompt: auto (let server optimize) or manual (custom content).
 * Discriminated union ensures content only exists for manual mode.
 */
export type PromptMode = { mode: "auto" } | { mode: "manual"; content: string };

/**
 * Configuration for a single prompt section.
 * Two-layer structure:
 * - enabled: Whether the section is active
 * - mode: The prompt mode (auto or manual with content)
 */
export interface PromptSection {
	enabled: boolean;
	mode: PromptMode;
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
	stt_provider: STTProviderId;
	llm_provider: LLMProviderId;
	auto_mute_audio: boolean;
	stt_timeout_seconds: number | null;
	server_url: string;
	/** LLM formatting enabled (true = format with LLM, false = raw transcription) */
	llm_formatting_enabled: boolean;
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

	async updateLLMFormattingEnabled(enabled: boolean): Promise<void> {
		return invoke("update_llm_formatting_enabled", { enabled });
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

	async addHistoryEntry(text: string, rawText: string): Promise<HistoryEntry> {
		return invoke("add_history_entry", { text, rawText });
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

	// LLM error notifications (overlay -> main)
	async emitLLMError(error: LLMErrorPayload): Promise<void> {
		return emitEvent(AppEvents.llmError, error);
	},

	async onLLMError(
		callback: (error: LLMErrorPayload) => void,
	): Promise<UnlistenFn> {
		return listenEvent(AppEvents.llmError, callback);
	},

	// Provider change requests (main -> overlay, pessimistic updates)
	async emitProviderChangeRequest(
		payload: ProviderChangeRequestPayload,
	): Promise<void> {
		return emitEvent(AppEvents.providerChangeRequest, payload);
	},

	async onProviderChangeRequest(
		callback: (payload: ProviderChangeRequestPayload) => void,
	): Promise<UnlistenFn> {
		return listenEvent(AppEvents.providerChangeRequest, callback);
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

	// Export/Import API
	async generateSettingsExport(): Promise<string> {
		return invoke("generate_settings_export");
	},

	async generateHistoryExport(): Promise<string> {
		return invoke("generate_history_export");
	},

	/** Generate prompt exports as markdown content. Returns map of section name -> markdown content. */
	async generatePromptExports(): Promise<Record<PromptSectionName, string>> {
		return invoke("generate_prompt_exports");
	},

	/** Parse a prompt file and extract section name and content from HTML comment header. */
	async parsePromptFile(content: string): Promise<[PromptSectionName, string]> {
		return invoke("parse_prompt_file", { content });
	},

	/** Import a prompt into the specified section. */
	async importPrompt(
		section: PromptSectionName,
		content: string,
	): Promise<void> {
		return invoke("import_prompt", { section, content });
	},

	async detectExportFileType(content: string): Promise<DetectedFileType> {
		return invoke("detect_export_file_type", { content });
	},

	async importSettings(content: string): Promise<void> {
		return invoke("import_settings", { content });
	},

	async importHistory(
		content: string,
		strategy: HistoryImportStrategy,
	): Promise<HistoryImportResult> {
		return invoke("import_history", { content, strategy });
	},

	async factoryReset(): Promise<void> {
		return invoke("factory_reset");
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
		prefixUrl: withoutTrailingSlash(serverUrl),
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
