/**
 * Type-safe event system for inter-window communication.
 *
 * Events are broadcast to all windows via Tauri's event system.
 * This module provides type-safe wrappers around emit/listen.
 *
 * IMPORTANT: Event names and payload types must match the Rust side.
 * See: src-tauri/src/events.rs
 */

import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ActiveAppContextSnapshot } from "./activeAppContext";

// =============================================================================
// Event Names - Must match src-tauri/src/events.rs
// =============================================================================

export const AppEvents = {
	// Rust → All: Hotkey triggers
	recordingStart: "recording-start",
	recordingStop: "recording-stop",
	prepareRecording: "prepare-recording",

	// Rust → All: Config sync notifications
	configResponse: "config-response",

	// Rust → Overlay: Disconnect request on app quit
	requestDisconnect: "request-disconnect",

	// Main → Overlay: Settings changed, refetch needed
	settingsChanged: "settings-changed",

	// Main → Overlay: Request reconnection
	reconnectRequest: "request-reconnect",

	// Overlay → Main: Connection state updates
	connectionState: "connection-state-changed",

	// Overlay → Main: Reconnection progress
	reconnectStarted: "reconnect-started",
	reconnectResult: "reconnect-result",

	// Rust → All: History changed
	historyChanged: "history-changed",

	// Overlay → Main: LLM error notification
	llmError: "llm-error",

	// Main → Overlay: Provider change request (pessimistic updates)
	providerChangeRequest: "provider-change-request",

	// Rust → All: Active app context updates
	activeAppContextChanged: "active-app-context-changed",
} as const;

// =============================================================================
// Event Payloads - Must match src-tauri/src/events.rs
// =============================================================================

export type ConnectionState =
	| "disconnected"
	| "connecting"
	| "reconnecting"
	| "idle"
	| "recording"
	| "processing";

/**
 * Known config setting names.
 * Unknown strings are still allowed for forward compatibility.
 */
export const KNOWN_CONFIG_SETTING_NAMES = [
	"stt-provider",
	"llm-provider",
	"prompt-sections",
	"stt-timeout",
] as const;

export type KnownConfigSettingName =
	(typeof KNOWN_CONFIG_SETTING_NAMES)[number];
export type ConfigSettingName = KnownConfigSettingName | (string & {});

/**
 * Config response for successful updates.
 * Value is parsed at runtime using Zod schemas in tauri.ts.
 */
export type ConfigUpdatedResponse = {
	type: "config-updated";
	setting: ConfigSettingName;
	value: unknown;
};

/**
 * Config error response.
 */
export type ConfigErrorResponse = {
	type: "config-error";
	setting: ConfigSettingName;
	error: string;
};

export type ConfigResponse = ConfigUpdatedResponse | ConfigErrorResponse;

export interface LLMErrorPayload {
	message: string; // Full error message for toast
	fatal: boolean;
}

export interface ProviderChangeRequestPayload {
	providerType: "stt" | "llm";
	value: string;
}

export interface EventPayloads {
	[AppEvents.recordingStart]: undefined;
	[AppEvents.recordingStop]: undefined;
	[AppEvents.prepareRecording]: undefined;
	[AppEvents.configResponse]: ConfigResponse;
	[AppEvents.requestDisconnect]: undefined;
	[AppEvents.settingsChanged]: undefined;
	[AppEvents.reconnectRequest]: undefined;
	[AppEvents.connectionState]: { state: ConnectionState };
	[AppEvents.reconnectStarted]: undefined;
	[AppEvents.reconnectResult]: { success: boolean; error?: string };
	[AppEvents.historyChanged]: undefined;
	[AppEvents.llmError]: LLMErrorPayload;
	[AppEvents.providerChangeRequest]: ProviderChangeRequestPayload;
	[AppEvents.activeAppContextChanged]: ActiveAppContextSnapshot;
}

// =============================================================================
// Type-safe emit/listen functions
// =============================================================================

/**
 * Emit an event with type-safe payload.
 */
export function emitEvent<K extends keyof EventPayloads>(
	event: K,
	...args: EventPayloads[K] extends undefined ? [] : [EventPayloads[K]]
): Promise<void> {
	return emit(event, args[0] ?? {});
}

/**
 * Listen for an event with type-safe callback.
 */
export function listenEvent<K extends keyof EventPayloads>(
	event: K,
	callback: (payload: EventPayloads[K]) => void,
): Promise<UnlistenFn> {
	return listen<EventPayloads[K]>(event, (eventPayload) =>
		callback(eventPayload.payload),
	);
}
