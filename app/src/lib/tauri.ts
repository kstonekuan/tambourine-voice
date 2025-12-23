import { invoke } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Store } from "@tauri-apps/plugin-store";
import ky from "ky";
import { z } from "zod";

export type ConnectionState =
	| "disconnected"
	| "connecting"
	| "idle"
	| "recording"
	| "processing";

export interface ConfigResponse {
	type: "config-updated" | "config-error";
	setting: string;
	value?: unknown;
	error?: string;
}

interface TypeTextResult {
	success: boolean;
	error?: string;
}

export interface HotkeyConfig {
	modifiers: string[];
	key: string;
}

// Zod schema for HotkeyConfig validation
export const HotkeyConfigSchema = z.object({
	modifiers: z.array(z.string()),
	key: z.string().min(1, "Key is required"),
});

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
}

// ============================================================================
// Default values - must match Rust defaults
// ============================================================================

const DEFAULT_HOTKEY_MODIFIERS = ["ctrl", "alt"];

export const defaultToggleHotkey: HotkeyConfig = {
	modifiers: DEFAULT_HOTKEY_MODIFIERS,
	key: "Space",
};

export const defaultHoldHotkey: HotkeyConfig = {
	modifiers: DEFAULT_HOTKEY_MODIFIERS,
	key: "Backquote",
};

export const defaultPasteLastHotkey: HotkeyConfig = {
	modifiers: DEFAULT_HOTKEY_MODIFIERS,
	key: "Period",
};

// ============================================================================
// Store helpers
// ============================================================================

let storeInstance: Store | null = null;

async function getStore(): Promise<Store> {
	if (!storeInstance) {
		storeInstance = await Store.load("settings.json");
	}
	return storeInstance;
}

// ============================================================================
// Hotkey validation helpers (Zod-based)
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

type HotkeyType = "toggle" | "hold" | "paste_last";

const HOTKEY_LABELS: Record<HotkeyType, string> = {
	toggle: "toggle",
	hold: "hold",
	paste_last: "paste last",
};

/**
 * Create a Zod schema for validating a hotkey doesn't conflict with existing hotkeys
 */
export function createHotkeyDuplicateSchema(
	allHotkeys: Record<HotkeyType, HotkeyConfig>,
	excludeType: HotkeyType,
) {
	return HotkeyConfigSchema.superRefine((hotkey, ctx) => {
		for (const [type, existing] of Object.entries(allHotkeys)) {
			if (type !== excludeType && hotkeyIsSameAs(hotkey, existing)) {
				ctx.addIssue({
					code: "custom",
					message: `This shortcut is already used for the ${HOTKEY_LABELS[type as HotkeyType]} hotkey`,
				});
				return;
			}
		}
	});
}

/**
 * Validate that a hotkey doesn't conflict with other hotkeys
 * Returns error message if invalid, null if valid
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
	const schema = createHotkeyDuplicateSchema(allHotkeys, excludeType);
	const result = schema.safeParse(newHotkey);
	if (!result.success) {
		return result.error.issues[0]?.message ?? "Invalid hotkey";
	}
	return null;
}

// ============================================================================
// Tauri API
// ============================================================================

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

	async onStartRecording(callback: () => void): Promise<UnlistenFn> {
		return listen("recording-start", callback);
	},

	async onStopRecording(callback: () => void): Promise<UnlistenFn> {
		return listen("recording-stop", callback);
	},

	// Settings API - using store plugin directly
	async getSettings(): Promise<AppSettings> {
		const store = await getStore();
		return {
			toggle_hotkey:
				(await store.get<HotkeyConfig>("toggle_hotkey")) ?? defaultToggleHotkey,
			hold_hotkey:
				(await store.get<HotkeyConfig>("hold_hotkey")) ?? defaultHoldHotkey,
			paste_last_hotkey:
				(await store.get<HotkeyConfig>("paste_last_hotkey")) ??
				defaultPasteLastHotkey,
			selected_mic_id:
				(await store.get<string | null>("selected_mic_id")) ?? null,
			sound_enabled: (await store.get<boolean>("sound_enabled")) ?? true,
			cleanup_prompt_sections:
				(await store.get<CleanupPromptSections | null>(
					"cleanup_prompt_sections",
				)) ?? null,
			stt_provider: (await store.get<string | null>("stt_provider")) ?? null,
			llm_provider: (await store.get<string | null>("llm_provider")) ?? null,
			auto_mute_audio: (await store.get<boolean>("auto_mute_audio")) ?? false,
			stt_timeout_seconds:
				(await store.get<number | null>("stt_timeout_seconds")) ?? null,
		};
	},

	async updateToggleHotkey(hotkey: HotkeyConfig): Promise<void> {
		const store = await getStore();
		await store.set("toggle_hotkey", hotkey);
		await store.save();
	},

	async updateHoldHotkey(hotkey: HotkeyConfig): Promise<void> {
		const store = await getStore();
		await store.set("hold_hotkey", hotkey);
		await store.save();
	},

	async updatePasteLastHotkey(hotkey: HotkeyConfig): Promise<void> {
		const store = await getStore();
		await store.set("paste_last_hotkey", hotkey);
		await store.save();
	},

	async updateSelectedMic(micId: string | null): Promise<void> {
		const store = await getStore();
		await store.set("selected_mic_id", micId);
		await store.save();
	},

	async updateSoundEnabled(enabled: boolean): Promise<void> {
		const store = await getStore();
		await store.set("sound_enabled", enabled);
		await store.save();
	},

	async updateCleanupPromptSections(
		sections: CleanupPromptSections | null,
	): Promise<void> {
		const store = await getStore();
		await store.set("cleanup_prompt_sections", sections);
		await store.save();
	},

	async updateSTTProvider(provider: string | null): Promise<void> {
		const store = await getStore();
		await store.set("stt_provider", provider);
		await store.save();
	},

	async updateLLMProvider(provider: string | null): Promise<void> {
		const store = await getStore();
		await store.set("llm_provider", provider);
		await store.save();
	},

	async updateAutoMuteAudio(enabled: boolean): Promise<void> {
		const store = await getStore();
		await store.set("auto_mute_audio", enabled);
		await store.save();
	},

	async updateSTTTimeout(timeoutSeconds: number | null): Promise<void> {
		const store = await getStore();
		await store.set("stt_timeout_seconds", timeoutSeconds);
		await store.save();
	},

	async isAudioMuteSupported(): Promise<boolean> {
		return invoke("is_audio_mute_supported");
	},

	async resetHotkeysToDefaults(): Promise<void> {
		const store = await getStore();
		await store.set("toggle_hotkey", defaultToggleHotkey);
		await store.set("hold_hotkey", defaultHoldHotkey);
		await store.set("paste_last_hotkey", defaultPasteLastHotkey);
		await store.save();
	},

	async registerShortcuts(): Promise<void> {
		return invoke("register_shortcuts");
	},

	async unregisterShortcuts(): Promise<void> {
		return invoke("unregister_shortcuts");
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

// ============================================================================
// Config API (FastAPI backend) - using ky HTTP client
// ============================================================================

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

// Create ky instance with sensible defaults for local API
const api = ky.create({
	prefixUrl: "http://127.0.0.1:8765",
	timeout: 10000,
	retry: {
		limit: 2,
		methods: ["get", "post"],
	},
});

export const configAPI = {
	// Static prompt defaults (runtime config goes via data channel)
	getDefaultSections: () =>
		api.get("api/prompt/sections/default").json<DefaultSectionsResponse>(),
	// Note: Provider info now comes via RTVI message after WebRTC connection
	// Use tauriAPI.onAvailableProviders() to listen for provider data
};
