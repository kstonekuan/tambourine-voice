import { invoke } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type ConnectionState =
	| "disconnected"
	| "connecting"
	| "idle"
	| "recording"
	| "processing";

interface TypeTextResult {
	success: boolean;
	error?: string;
}

export interface HotkeyConfig {
	modifiers: string[];
	key: string;
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

	async onStartRecording(callback: () => void): Promise<UnlistenFn> {
		return listen("recording-start", callback);
	},

	async onStopRecording(callback: () => void): Promise<UnlistenFn> {
		return listen("recording-stop", callback);
	},

	// Settings API
	async getSettings(): Promise<AppSettings> {
		return invoke("get_settings");
	},

	async updateToggleHotkey(hotkey: HotkeyConfig): Promise<void> {
		return invoke("update_toggle_hotkey", { hotkey });
	},

	async updateHoldHotkey(hotkey: HotkeyConfig): Promise<void> {
		return invoke("update_hold_hotkey", { hotkey });
	},

	async updatePasteLastHotkey(hotkey: HotkeyConfig): Promise<void> {
		return invoke("update_paste_last_hotkey", { hotkey });
	},

	async updateToggleHotkeyLive(hotkey: HotkeyConfig): Promise<void> {
		return invoke("update_toggle_hotkey_live", { hotkey });
	},

	async updateHoldHotkeyLive(hotkey: HotkeyConfig): Promise<void> {
		return invoke("update_hold_hotkey_live", { hotkey });
	},

	async updatePasteLastHotkeyLive(hotkey: HotkeyConfig): Promise<void> {
		return invoke("update_paste_last_hotkey_live", { hotkey });
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

	async isAudioMuteSupported(): Promise<boolean> {
		return invoke("is_audio_mute_supported");
	},

	async resetHotkeysToDefaults(): Promise<boolean> {
		return invoke("reset_hotkeys_to_defaults");
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
};

// Config API for server-side settings (FastAPI)
// Now merged with main server (WebRTC + Config on same port)
const CONFIG_API_URL = "http://127.0.0.1:8765";

export interface DefaultSectionsResponse {
	main: string;
	advanced: string;
	dictionary: string;
}

interface SetPromptResponse {
	success: boolean;
	error?: string;
}

interface ProviderInfo {
	value: string;
	label: string;
}

interface AvailableProvidersResponse {
	stt: ProviderInfo[];
	llm: ProviderInfo[];
}

interface CurrentProvidersResponse {
	stt: string | null;
	llm: string | null;
}

interface SwitchProviderResponse {
	success: boolean;
	provider?: string;
	error?: string;
}

interface STTTimeoutResponse {
	success: boolean;
	timeout_seconds?: number;
	error?: string;
}

export const configAPI = {
	async getDefaultSections(): Promise<DefaultSectionsResponse> {
		const response = await fetch(
			`${CONFIG_API_URL}/api/prompt/sections/default`,
		);
		return response.json();
	},

	async setPromptSections(
		sections: CleanupPromptSections,
	): Promise<SetPromptResponse> {
		const response = await fetch(`${CONFIG_API_URL}/api/prompt/sections`, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ sections }),
		});
		return response.json();
	},

	// Provider APIs
	async getAvailableProviders(): Promise<AvailableProvidersResponse> {
		const response = await fetch(`${CONFIG_API_URL}/api/providers/available`);
		return response.json();
	},

	async getCurrentProviders(): Promise<CurrentProvidersResponse> {
		const response = await fetch(`${CONFIG_API_URL}/api/providers/current`);
		return response.json();
	},

	async setSTTProvider(provider: string): Promise<SwitchProviderResponse> {
		const response = await fetch(`${CONFIG_API_URL}/api/providers/stt`, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ provider }),
		});
		return response.json();
	},

	async setLLMProvider(provider: string): Promise<SwitchProviderResponse> {
		const response = await fetch(`${CONFIG_API_URL}/api/providers/llm`, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ provider }),
		});
		return response.json();
	},

	// STT Timeout APIs
	async getSTTTimeout(): Promise<STTTimeoutResponse> {
		const response = await fetch(`${CONFIG_API_URL}/api/config/stt-timeout`);
		return response.json();
	},

	async setSTTTimeout(timeoutSeconds: number): Promise<STTTimeoutResponse> {
		const response = await fetch(`${CONFIG_API_URL}/api/config/stt-timeout`, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ timeout_seconds: timeoutSeconds }),
		});
		return response.json();
	},
};
