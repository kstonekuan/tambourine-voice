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

export type ConfigResponse =
	| {
			type: "config-updated";
			setting: string;
			value: unknown;
	  }
	| {
			type: "config-error";
			setting: string;
			error: string;
	  };

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
	callback: EventPayloads[K] extends undefined
		? () => void
		: (payload: EventPayloads[K]) => void,
): Promise<UnlistenFn> {
	return listen<EventPayloads[K]>(event, (e) => {
		(callback as (payload: EventPayloads[K]) => void)(e.payload);
	});
}
