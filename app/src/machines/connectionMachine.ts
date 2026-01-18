import { PipecatClient, RTVIEvent } from "@pipecat-ai/client-js";
import { SmallWebRTCTransport } from "@pipecat-ai/small-webrtc-transport";
import {
	type ActorRefFrom,
	assign,
	fromCallback,
	fromPromise,
	setup,
} from "xstate";
import { type ConnectionState, tauriAPI } from "../lib/tauri";

/**
 * XState-based connection state machine for managing PipecatClient lifecycle.
 *
 * This machine handles:
 * - Initial connection establishment
 * - Automatic reconnection with exponential backoff
 * - Clean state transitions that prevent race conditions
 * - Proper cleanup of client resources
 */

// Context type for the state machine
interface ConnectionContext {
	client: PipecatClient | null;
	serverUrl: string;
	retryCount: number;
	error: string | null;
}

// Events that can be sent to the machine
type ConnectionEvents =
	| { type: "CONNECT"; serverUrl: string }
	| { type: "CLIENT_READY"; client: PipecatClient }
	| { type: "CLIENT_ERROR"; error: string }
	| { type: "CONNECTED" }
	| { type: "DISCONNECTED" }
	| { type: "RECONNECT" }
	| { type: "START_RECORDING" }
	| { type: "STOP_RECORDING" }
	| { type: "RESPONSE_RECEIVED" }
	| { type: "SERVER_URL_CHANGED"; serverUrl: string }
	| { type: "COMMUNICATION_ERROR"; error: string };

// Actor that creates a fresh PipecatClient instance
const createClientActor = fromPromise<PipecatClient, void>(async () => {
	const transport = new SmallWebRTCTransport({
		iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
	});
	const client = new PipecatClient({
		transport,
		enableMic: false,
		enableCam: false,
	});

	await client.initDevices();

	// Release mic after device enumeration to avoid keeping it open
	try {
		const tracks = client.tracks();
		if (tracks?.local?.audio) {
			tracks.local.audio.stop();
		}
	} catch {
		// Ignore cleanup errors
	}

	return client;
});

/**
 * Actor that initiates connection and listens for transport state changes.
 * Used ONLY in the 'connecting' state - calls client.connect().
 *
 * Waits for transport to reach "ready" state (not just "connected") because:
 * - RTVIEvent.Connected fires when WebRTC connection is established ("connected" state)
 * - sendClientMessage() requires the data channel which is only available in "ready" state
 * - This gap caused "transport not in ready state" errors
 */
const connectActor = fromCallback<
	{ type: "CONNECTED" } | { type: "DISCONNECTED" },
	{ client: PipecatClient; serverUrl: string }
>(({ sendBack, input }) => {
	const { client, serverUrl } = input;

	const handleTransportStateChanged = (state: string) => {
		console.debug("[XState] Transport state changed:", state);
		if (state === "ready") {
			console.debug("[XState] PipecatClient ready for messages");
			sendBack({ type: "CONNECTED" });
		}
	};

	const handleDisconnected = () => {
		console.debug("[XState] PipecatClient disconnected (during connect)");
		sendBack({ type: "DISCONNECTED" });
	};

	// Subscribe to transport state changes (not just Connected event)
	// This ensures we wait for "ready" state before transitioning to idle
	client.on(RTVIEvent.TransportStateChanged, handleTransportStateChanged);
	client.on(RTVIEvent.Disconnected, handleDisconnected);

	// Start connection (fire-and-forget, events will drive state)
	client
		.connect({
			webrtcRequestParams: { endpoint: `${serverUrl}/api/offer` },
		})
		.catch((error) => {
			console.error("[XState] Connection error:", error);
			// Connection errors will eventually trigger a disconnect event
		});

	// Cleanup function - remove event listeners when state exits
	return () => {
		client.off(RTVIEvent.TransportStateChanged, handleTransportStateChanged);
		client.off(RTVIEvent.Disconnected, handleDisconnected);
	};
});

/**
 * Actor that listens for disconnect events and transport state degradation.
 * Used in 'idle', 'recording', and 'processing' states to detect:
 * - Server disconnection (RTVIEvent.Disconnected)
 * - Stale connections after sleep/wake (transport state drops from "ready")
 *
 * WebRTC connections often become stale during system sleep but may not fire
 * clean disconnect events. By monitoring transport state, we can detect when
 * the connection degrades and trigger reconnection proactively.
 */
const disconnectListenerActor = fromCallback<
	{ type: "DISCONNECTED" },
	{ client: PipecatClient }
>(({ sendBack, input }) => {
	const { client } = input;

	const handleDisconnected = () => {
		console.debug("[XState] PipecatClient disconnected");
		sendBack({ type: "DISCONNECTED" });
	};

	const handleTransportStateChanged = (state: string) => {
		// If transport drops out of "ready" state, treat as disconnection
		// This catches stale connections after sleep/wake
		if (state !== "ready" && state !== "connected") {
			console.debug("[XState] Transport state degraded:", state);
			sendBack({ type: "DISCONNECTED" });
		}
	};

	// Subscribe to both disconnect and transport state changes
	client.on(RTVIEvent.Disconnected, handleDisconnected);
	client.on(RTVIEvent.TransportStateChanged, handleTransportStateChanged);

	// Cleanup function
	return () => {
		client.off(RTVIEvent.Disconnected, handleDisconnected);
		client.off(RTVIEvent.TransportStateChanged, handleTransportStateChanged);
	};
});

export const connectionMachine = setup({
	types: {
		context: {} as ConnectionContext,
		events: {} as ConnectionEvents,
	},
	actors: {
		createClient: createClientActor,
		connect: connectActor,
		disconnectListener: disconnectListenerActor,
	},
	actions: {
		// Emit connection state to main window via Tauri events
		emitConnectionState: (_, params: { state: ConnectionState }): void => {
			tauriAPI.emitConnectionState(params.state);
		},
		emitReconnectStarted: (): void => {
			tauriAPI.emitReconnectStarted();
		},
		emitReconnectResult: (
			_,
			params: { success: boolean; error?: string },
		): void => {
			tauriAPI.emitReconnectResult(params.success, params.error);
		},
		// Clean up client resources (mic, tracks, connection)
		// IMPORTANT: Close the RTCPeerConnection directly first to prevent the library's
		// internal event handlers from trying to send messages during graceful shutdown.
		// Without this, track-ended events trigger sends on an already-closing data channel,
		// causing InvalidStateError in the library code.
		cleanupClient: ({ context }): void => {
			if (context.client) {
				try {
					// Close peer connection immediately to prevent internal sends
					const transport = context.client.transport as SmallWebRTCTransport;
					const peerConnection = (
						transport as unknown as { pc?: RTCPeerConnection }
					).pc;
					if (peerConnection) {
						peerConnection.close();
					}
				} catch {
					// Ignore errors - peer connection may not exist yet
				}
				// Still call disconnect for any remaining cleanup (state reset, etc.)
				context.client.disconnect().catch(() => {});
			}
		},
		logState: (_, params: { state: string }): void => {
			console.log(`[XState] → ${params.state}`);
		},
	},
	guards: {
		canRetry: ({ context }) => context.retryCount < 10,
	},
	delays: {
		connectionTimeout: 30000,
		// Exponential backoff: 1s, 2s, 4s, 8s... capped at 30s
		retryDelay: ({ context }) =>
			Math.min(1000 * 2 ** context.retryCount, 30000),
	},
}).createMachine({
	id: "connection",
	initial: "disconnected",
	context: {
		client: null,
		serverUrl: "",
		retryCount: 0,
		error: null,
	},

	states: {
		disconnected: {
			entry: [
				{ type: "emitConnectionState", params: { state: "disconnected" } },
				{ type: "logState", params: { state: "disconnected" } },
			],
			on: {
				CONNECT: {
					target: "initializing",
					actions: assign({ serverUrl: ({ event }) => event.serverUrl }),
				},
			},
		},

		// Create a fresh PipecatClient before attempting connection
		initializing: {
			entry: [
				{ type: "emitConnectionState", params: { state: "connecting" } },
				{ type: "logState", params: { state: "initializing" } },
			],
			invoke: {
				src: "createClient",
				onDone: {
					target: "connecting",
					actions: assign({ client: ({ event }) => event.output }),
				},
				onError: {
					target: "retrying",
					actions: assign({
						error: ({ event }) =>
							event.error instanceof Error
								? event.error.message
								: String(event.error),
					}),
				},
			},
		},

		// Connect the client to the server
		connecting: {
			entry: [{ type: "logState", params: { state: "connecting" } }],
			invoke: {
				// Use connect actor which initiates the connection
				src: "connect",
				input: ({ context }) => ({
					client: context.client as PipecatClient,
					serverUrl: context.serverUrl,
				}),
			},
			on: {
				CONNECTED: {
					target: "idle",
					actions: assign({ retryCount: 0, error: null }),
				},
				DISCONNECTED: "retrying",
			},
			after: {
				connectionTimeout: {
					target: "retrying",
					actions: assign({ error: () => "Connection timeout" }),
				},
			},
		},

		// Connected and ready for recording
		idle: {
			entry: [
				{ type: "emitConnectionState", params: { state: "idle" } },
				{ type: "emitReconnectResult", params: { success: true } },
				{ type: "logState", params: { state: "idle" } },
			],
			invoke: {
				// Use disconnect listener - does NOT call connect()
				src: "disconnectListener",
				input: ({ context }) => ({
					client: context.client as PipecatClient,
				}),
			},
			on: {
				DISCONNECTED: "retrying",
				COMMUNICATION_ERROR: {
					target: "retrying",
					actions: "cleanupClient",
				},
				START_RECORDING: "recording",
				SERVER_URL_CHANGED: {
					target: "initializing",
					actions: [
						"cleanupClient",
						assign({
							serverUrl: ({ event }) => event.serverUrl,
							client: () => null,
							retryCount: () => 0,
						}),
					],
				},
				RECONNECT: {
					target: "initializing",
					actions: [
						"cleanupClient",
						"emitReconnectStarted",
						assign({ client: () => null, retryCount: () => 0 }),
					],
				},
			},
		},

		// Actively recording audio
		recording: {
			entry: [
				{ type: "emitConnectionState", params: { state: "recording" } },
				{ type: "logState", params: { state: "recording" } },
			],
			invoke: {
				// Use disconnect listener - does NOT call connect()
				src: "disconnectListener",
				input: ({ context }) => ({
					client: context.client as PipecatClient,
				}),
			},
			on: {
				DISCONNECTED: {
					target: "retrying",
					actions: "cleanupClient",
				},
				COMMUNICATION_ERROR: {
					target: "retrying",
					actions: "cleanupClient",
				},
				STOP_RECORDING: "processing",
			},
		},

		// Waiting for server to process and respond
		processing: {
			entry: [
				{ type: "emitConnectionState", params: { state: "processing" } },
				{ type: "logState", params: { state: "processing" } },
			],
			invoke: {
				// Use disconnect listener - does NOT call connect()
				src: "disconnectListener",
				input: ({ context }) => ({
					client: context.client as PipecatClient,
				}),
			},
			on: {
				DISCONNECTED: {
					target: "retrying",
					actions: "cleanupClient",
				},
				COMMUNICATION_ERROR: {
					target: "retrying",
					actions: "cleanupClient",
				},
				RESPONSE_RECEIVED: "idle",
			},
		},

		// Reconnecting with exponential backoff
		retrying: {
			entry: [
				{ type: "emitConnectionState", params: { state: "reconnecting" } },
				"emitReconnectStarted",
				"cleanupClient",
				assign({
					retryCount: ({ context }) => context.retryCount + 1,
					client: () => null,
				}),
				{ type: "logState", params: { state: "retrying" } },
			],
			after: {
				retryDelay: [
					{ target: "initializing", guard: "canRetry" },
					{
						target: "disconnected",
						actions: [
							{
								type: "emitReconnectResult",
								params: { success: false, error: "Max retries exceeded" },
							},
							assign({ error: () => "Max retries exceeded" }),
						],
					},
				],
			},
			on: {
				// Manual reconnect resets retry counter and retries immediately
				RECONNECT: {
					target: "initializing",
					actions: assign({ retryCount: () => 0 }),
				},
			},
		},
	},
});

// Export types for consumers
export type ConnectionMachineActor = ActorRefFrom<typeof connectionMachine>;
export type { ConnectionContext, ConnectionEvents };
