import type { PipecatClient } from "@pipecat-ai/client-js";
import { create } from "zustand";

/**
 * Explicit state machine for connection and recording states.
 * Prevents invalid state combinations (e.g., recording while disconnected).
 */
type ConnectionState =
	| "disconnected" // Not connected to server
	| "connecting" // Connection in progress
	| "idle" // Connected, ready to record
	| "recording" // Mic enabled, streaming audio
	| "processing"; // Waiting for server response

interface RecordingState {
	state: ConnectionState;
	client: PipecatClient | null;

	// Actions
	setClient: (client: PipecatClient | null) => void;
	setState: (state: ConnectionState) => void;

	// State transitions
	handleConnected: () => void;
	handleDisconnected: () => void;
	startRecording: () => Promise<boolean>; // Returns false if not in valid state
	stopRecording: () => boolean; // Returns false if not in valid state
	handleResponse: () => void;

	// Configuration via data channel
	sendConfigMessage: (type: string, data: unknown) => boolean;
}

export const useRecordingStore = create<RecordingState>((set, get) => ({
	state: "disconnected",
	client: null,

	setClient: (client) => set({ client }),
	setState: (state) => set({ state }),

	handleConnected: () => {
		const currentState = get().state;
		if (currentState === "connecting" || currentState === "disconnected") {
			set({ state: "idle" });
		}
	},

	handleDisconnected: () => {
		// Only reset connection state - keep client reference since we reuse it for reconnection
		set({ state: "disconnected" });
	},

	startRecording: async () => {
		const { state, client } = get();
		if (state !== "idle" || !client) {
			return false;
		}

		// Signal server to reset buffer and enable mic
		try {
			client.sendClientMessage("start-recording", {});

			// Note: updateMic() is called in onStartRecording before this function,
			// so the mic track is already acquired by the time we get here

			client.enableMic(true);
			set({ state: "recording" });
			return true;
		} catch (error) {
			console.error("[Recording] Error starting:", error);
			return false;
		}
	},

	stopRecording: () => {
		const { state, client } = get();
		if (state !== "recording" || !client) {
			return false;
		}

		// Disable mic first
		try {
			client.enableMic(false);
		} catch (error) {
			console.warn("[Recording] Failed to disable mic:", error);
		}

		// Stop the audio track immediately to release the microphone (removes OS mic indicator)
		// This must happen here, not in handleResponse(), so the mic is released even if
		// the server is slow to respond. updateMic() will re-acquire when starting next recording.
		try {
			const tracks = client.tracks();
			if (tracks?.local?.audio) {
				tracks.local.audio.stop();
			}
		} catch (error) {
			console.warn("[Recording] Failed to stop audio track:", error);
		}

		// Try to send stop message to server
		try {
			client.sendClientMessage("stop-recording", {});
			set({ state: "processing" });
			return true;
		} catch (error) {
			console.warn("[Recording] Failed to send stop message:", error);
			set({ state: "disconnected" });
			return true;
		}
	},

	handleResponse: () => {
		const { state } = get();
		if (state === "processing") {
			// Track is already stopped in stopRecording(), just transition state
			set({ state: "idle" });
		}
	},

	sendConfigMessage: (type: string, data: unknown) => {
		const { state, client } = get();
		// Only send if connected (idle, recording, or processing)
		if (state === "disconnected" || state === "connecting" || !client) {
			console.warn(
				`[Config] Cannot send message in state: ${state}, client: ${!!client}`,
			);
			return false;
		}

		try {
			client.sendClientMessage(type, data);
			return true;
		} catch (error) {
			console.error("[Config] Failed to send message:", error);
			return false;
		}
	},
}));
