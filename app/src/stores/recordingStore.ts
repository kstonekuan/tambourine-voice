import { create } from "zustand";

/**
 * Simple UI state store for connection status display.
 *
 * This store is used by the main window (App.tsx, ConnectionSettings.tsx) to track
 * connection state received from the overlay window via Tauri events.
 *
 * The actual connection logic is managed by the XState machine in the overlay window.
 * This store just mirrors that state for UI display purposes.
 */
type ConnectionState =
	| "disconnected"
	| "connecting"
	| "reconnecting"
	| "idle"
	| "recording"
	| "processing";

interface RecordingState {
	state: ConnectionState;
	setState: (state: ConnectionState) => void;
}

export const useRecordingStore = create<RecordingState>((set) => ({
	state: "disconnected",
	setState: (state) => set({ state }),
}));
