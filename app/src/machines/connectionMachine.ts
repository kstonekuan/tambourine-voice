import { PipecatClient, RTVIEvent } from "@pipecat-ai/client-js";
import { SmallWebRTCTransport } from "@pipecat-ai/small-webrtc-transport";
import { match } from "ts-pattern";
import { joinURL } from "ufo";
import {
	type ActorRefFrom,
	type AnyEventObject,
	assign,
	fromCallback,
	fromPromise,
	type StateValueFrom,
	setup,
} from "xstate";
import type { ProviderChangeRequestPayload } from "../lib/events";
import { isMacOSRuntime } from "../lib/runtimePlatform";
import {
	type ConfigMessage,
	sendConfigMessages,
} from "../lib/safeSendClientMessage";
import {
	type ConnectionState,
	configAPI,
	tauriAPI,
	toLLMProviderSelection,
	toSTTProviderSelection,
} from "../lib/tauri";

// Connection timing constants
const CONNECTION_TIMEOUT_MS = 30000;
const INITIAL_RETRY_DELAY_MS = 1000;
const MAX_RETRY_DELAY_MS = 30000;

/**
 * Clears the transport's keepAliveInterval to prevent "InvalidStateError" spam.
 * The library's stop() has a bug where the interval isn't cleared on abrupt disconnects.
 */
function clearKeepAliveInterval(client: PipecatClient): void {
	const transport = client.transport as { keepAliveInterval?: NodeJS.Timeout };
	if (transport?.keepAliveInterval) {
		clearInterval(transport.keepAliveInterval);
		transport.keepAliveInterval = undefined;
	}
}

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
	clientUUID: string | null;
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
	| { type: "START_RECORDING_READY" }
	| { type: "START_RECORDING_FAILED"; error: string }
	| { type: "STOP_RECORDING" }
	| { type: "RESPONSE_RECEIVED" }
	| { type: "SERVER_URL_CHANGED"; serverUrl: string }
	| { type: "COMMUNICATION_ERROR"; error: string }
	| { type: "UUID_REJECTED" };

// Actor that creates a fresh PipecatClient instance and ensures UUID is registered
const createClientActor = fromPromise<
	{ client: PipecatClient; clientUUID: string },
	{ serverUrl: string }
>(async ({ input }) => {
	const { serverUrl } = input;

	// Ensure we have a registered UUID (register if needed, verify if exists)
	let clientUUID = await tauriAPI.getClientUUID();
	if (clientUUID) {
		// Verify stored UUID is still registered with the server
		// (server may have restarted, losing in-memory registrations)
		try {
			const isRegistered = await configAPI.verifyClient(serverUrl, clientUUID);
			if (!isRegistered) {
				console.debug(
					"[XState] Stored UUID no longer registered, will re-register",
				);
				await tauriAPI.clearClientUUID();
				clientUUID = null;
			} else {
				console.debug(
					"[XState] Verified stored UUID is registered:",
					clientUUID,
				);
			}
		} catch (error) {
			console.warn("[XState] Failed to verify UUID, will re-register:", error);
			await tauriAPI.clearClientUUID();
			clientUUID = null;
		}
	}

	if (!clientUUID) {
		console.debug("[XState] Registering new UUID with server");
		clientUUID = await configAPI.registerClient(serverUrl);
		await tauriAPI.setClientUUID(clientUUID);
		console.debug("[XState] Registered and stored new UUID:", clientUUID);
	}

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

	return { client, clientUUID };
});

/**
 * Actor that initiates connection and listens for transport state changes.
 * Used ONLY in the 'connecting' state - calls client.connect().
 *
 * Waits for transport to reach "ready" state (not just "connected") because:
 * - RTVIEvent.Connected fires when WebRTC connection is established ("connected" state)
 * - sendClientMessage() requires the data channel which is only available in "ready" state
 * - This gap caused "transport not in ready state" errors
 *
 * Passes clientUUID in requestData for server-side client identification.
 * Handles 401 errors (unregistered UUID) by sending UUID_REJECTED event.
 */
const connectActor = fromCallback<
	{ type: "CONNECTED" } | { type: "DISCONNECTED" } | { type: "UUID_REJECTED" },
	{ client: PipecatClient; serverUrl: string; clientUUID: string }
>(({ sendBack, input }) => {
	const { client, serverUrl, clientUUID } = input;

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

	// Start connection with clientUUID in requestData
	client
		.connect({
			webrtcRequestParams: {
				endpoint: joinURL(serverUrl, "api/offer"),
				requestData: { clientUUID },
			},
		})
		.catch((error: unknown) => {
			console.error("[XState] Connection error:", error);
			console.debug(
				"[XState] Error details:",
				JSON.stringify(error, Object.getOwnPropertyNames(error)),
			);

			// Check for 401 (unregistered UUID) - server rejected our UUID
			// Try multiple error formats as different HTTP libraries structure errors differently
			const httpError = error as {
				response?: { status?: number };
				status?: number;
				message?: string;
			};
			const status = httpError?.response?.status ?? httpError?.status;
			const is401 = status === 401 || httpError?.message?.includes("401");

			if (is401) {
				console.warn(
					"[XState] UUID rejected by server (401), will re-register",
				);
				sendBack({ type: "UUID_REJECTED" });
				return;
			}
			// Other connection errors will eventually trigger a disconnect event
		});

	// Cleanup function - remove event listeners when state exits
	return () => {
		client.off(RTVIEvent.TransportStateChanged, handleTransportStateChanged);
		client.off(RTVIEvent.Disconnected, handleDisconnected);
	};
});

/**
 * Actor that listens for disconnect events and transport state degradation.
 * Used in connected runtime states where a client is active to detect:
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

	// Monitor peer connection state for data channel errors
	// RTCDataChannel failures during processing may not trigger RTVI events,
	// but they will cause the peer connection state to change to "failed" or "disconnected"
	const transport = client.transport as SmallWebRTCTransport;
	const peerConnection = (transport as unknown as { pc?: RTCPeerConnection })
		.pc;

	const handleConnectionStateChange = () => {
		if (!peerConnection) return;
		const state = peerConnection.connectionState;
		console.debug("[XState] Peer connection state:", state);

		if (state === "failed" || state === "disconnected") {
			console.warn(
				"[XState] Peer connection failed/disconnected, triggering reconnection",
			);
			sendBack({ type: "DISCONNECTED" });
		}
	};

	// Subscribe to disconnect, transport state changes, and peer connection state
	client.on(RTVIEvent.Disconnected, handleDisconnected);
	client.on(RTVIEvent.TransportStateChanged, handleTransportStateChanged);
	peerConnection?.addEventListener(
		"connectionstatechange",
		handleConnectionStateChange,
	);

	// Cleanup function
	return () => {
		client.off(RTVIEvent.Disconnected, handleDisconnected);
		client.off(RTVIEvent.TransportStateChanged, handleTransportStateChanged);
		peerConnection?.removeEventListener(
			"connectionstatechange",
			handleConnectionStateChange,
		);
	};
});

// =============================================================================
// Provider Change Listener Actor
// =============================================================================

/**
 * Maps provider type to the corresponding setting name for error reporting.
 */
function getSettingNameFromProviderType(
	providerType: "stt" | "llm",
): "stt-provider" | "llm-provider" {
	return providerType === "stt" ? "stt-provider" : "llm-provider";
}

/**
 * Actor that listens for provider change requests from the main window.
 * Only active in the 'idle' state when the client is connected and ready.
 *
 * This replaces the useEffect-based listener in OverlayApp.tsx which suffered
 * from stale closure issues - the callback would capture an old `client` reference
 * and report "Not connected" even when connected.
 *
 * Benefits of using an XState actor:
 * - Fresh client reference from machine context on each invocation
 * - Automatic cleanup when exiting idle state (no orphaned listeners)
 * - Guaranteed to only run when client is valid (idle state invariant)
 */
const providerChangeListenerActor = fromCallback<
	AnyEventObject, // Doesn't send events back to machine (but XState requires non-never type)
	{ client: PipecatClient }
>(({ input }) => {
	const { client } = input;

	const handleProviderChange = (payload: ProviderChangeRequestPayload) => {
		// Client is guaranteed non-null because we're in idle state
		const message: ConfigMessage = match(payload.providerType)
			.with("stt", () => ({
				type: "set-stt-provider" as const,
				data: { provider: toSTTProviderSelection(payload.value) },
			}))
			.with("llm", () => ({
				type: "set-llm-provider" as const,
				data: { provider: toLLMProviderSelection(payload.value) },
			}))
			.exhaustive();

		sendConfigMessages(client, [message], (error) => {
			tauriAPI.emitConfigResponse({
				type: "config-error",
				setting: getSettingNameFromProviderType(payload.providerType),
				error,
			});
		});
	};

	// Set up listener
	let unlisten: (() => void) | undefined;
	tauriAPI.onProviderChangeRequest(handleProviderChange).then((fn) => {
		unlisten = fn;
	});

	// Cleanup on state exit
	return () => {
		unlisten?.();
	};
});

// =============================================================================
// Initial Config Sync Actor
// =============================================================================

/**
 * Fire-and-forget actor that pushes the client's stored STT/LLM provider
 * selections to the server on connection establishment.
 *
 * Without this, a server restart causes the server to fall back to its defaults
 * because the providerChangeListenerActor only captures *future* user changes.
 */
const initialConfigSyncActor = fromPromise<void, { client: PipecatClient }>(
	async ({ input }) => {
		const { client } = input;

		const settings = await tauriAPI.getSettings();

		const messages: ConfigMessage[] = [
			{
				type: "set-stt-provider",
				data: { provider: toSTTProviderSelection(settings.stt_provider) },
			},
			{
				type: "set-llm-provider",
				data: { provider: toLLMProviderSelection(settings.llm_provider) },
			},
		];

		sendConfigMessages(client, messages);
		console.debug("[XState] Initial config sync sent to server");
	},
);

function assertClient(context: ConnectionContext): PipecatClient {
	if (!context.client) {
		throw new Error(
			"Invariant violation: context.client is null in a state that requires a connected client",
		);
	}
	return context.client;
}

function assertClientUUID(context: ConnectionContext): string {
	if (!context.clientUUID) {
		throw new Error(
			"Invariant violation: context.clientUUID is null in a state that requires a registered UUID",
		);
	}
	return context.clientUUID;
}

export const connectionMachine = setup({
	types: {
		context: {} as ConnectionContext,
		events: {} as ConnectionEvents,
	},
	actors: {
		createClient: createClientActor,
		connect: connectActor,
		disconnectListener: disconnectListenerActor,
		providerChangeListener: providerChangeListenerActor,
		initialConfigSync: initialConfigSyncActor,
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
		cleanupClient: ({ context }): void => {
			if (!context.client) return;
			// Clear the keepAliveInterval manually since the library's cleanup is buggy
			// (the "close" event never fires when the PC is already dead)
			clearKeepAliveInterval(context.client);
			context.client.disconnect().catch(() => {});
		},
		logState: (_, params: { state: string }): void => {
			console.log(`[XState] → ${params.state}`);
		},
		enableClientMicrophoneForRecordingIfSupported: ({ context }): void => {
			if (isMacOSRuntime()) {
				return;
			}

			if (!context.client) {
				return;
			}

			const clientWithOptionalEnableMic = context.client as PipecatClient & {
				enableMic?: (enabled: boolean) => void;
			};
			if (typeof clientWithOptionalEnableMic.enableMic !== "function") {
				return;
			}

			try {
				clientWithOptionalEnableMic.enableMic(true);
			} catch (error) {
				console.warn(
					"[Recording] Failed to enable Pipecat client microphone:",
					error,
				);
			}
		},
		disableClientMicrophoneAfterRecordingIfSupported: ({ context }): void => {
			if (isMacOSRuntime()) {
				return;
			}

			if (!context.client) {
				return;
			}

			const clientWithOptionalMicControls = context.client as PipecatClient & {
				enableMic?: (enabled: boolean) => void;
				tracks?: () => {
					local?: {
						audio?: {
							stop: () => void;
						};
					};
				};
			};

			try {
				clientWithOptionalMicControls.enableMic?.(false);
			} catch (error) {
				console.warn(
					"[Recording] Failed to disable Pipecat client microphone:",
					error,
				);
			}

			try {
				clientWithOptionalMicControls.tracks?.().local?.audio?.stop();
			} catch (error) {
				console.warn(
					"[Recording] Failed to stop Pipecat client local audio track:",
					error,
				);
			}
		},
	},
	delays: {
		connectionTimeout: CONNECTION_TIMEOUT_MS,
		// Exponential backoff: 1s, 2s, 4s, 8s... capped at 30s
		retryDelay: ({ context }) =>
			Math.min(
				INITIAL_RETRY_DELAY_MS * 2 ** context.retryCount,
				MAX_RETRY_DELAY_MS,
			),
	},
}).createMachine({
	id: "connection",
	initial: "disconnected",
	context: {
		client: null,
		clientUUID: null,
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

		// Create a fresh PipecatClient and ensure UUID is registered
		initializing: {
			entry: [
				{ type: "emitConnectionState", params: { state: "connecting" } },
				{ type: "logState", params: { state: "initializing" } },
			],
			invoke: {
				src: "createClient",
				input: ({ context }) => ({ serverUrl: context.serverUrl }),
				onDone: {
					target: "connecting",
					actions: assign({
						client: ({ event }) => event.output.client,
						clientUUID: ({ event }) => event.output.clientUUID,
					}),
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
				// Use connect actor which initiates the connection with clientUUID
				src: "connect",
				input: ({ context }) => ({
					client: assertClient(context),
					serverUrl: context.serverUrl,
					clientUUID: assertClientUUID(context),
				}),
			},
			on: {
				CONNECTED: {
					target: "syncing",
					actions: assign({ retryCount: 0, error: null }),
				},
				DISCONNECTED: "retrying",
				// UUID rejected by server (e.g., after server restart)
				// Clear stored UUID and go back to initializing to re-register
				UUID_REJECTED: {
					target: "initializing",
					actions: [
						"cleanupClient",
						async () => {
							await tauriAPI.clearClientUUID();
							console.debug("[XState] Cleared invalid UUID, will re-register");
						},
						assign({
							client: () => null,
							clientUUID: () => null,
						}),
					],
				},
			},
			after: {
				connectionTimeout: {
					target: "retrying",
					actions: assign({ error: () => "Connection timeout" }),
				},
			},
		},

		// Push stored provider selections to server after connecting
		syncing: {
			entry: [{ type: "logState", params: { state: "syncing" } }],
			invoke: [
				{
					// Monitor for disconnection events during sync
					src: "disconnectListener",
					input: ({ context }) => ({
						client: assertClient(context),
					}),
				},
				{
					src: "initialConfigSync",
					input: ({ context }) => ({
						client: assertClient(context),
					}),
					onDone: { target: "idle" },
					onError: { target: "idle" },
				},
			],
			on: {
				DISCONNECTED: "retrying",
				RECONNECT: {
					target: "initializing",
					actions: [
						"cleanupClient",
						"emitReconnectStarted",
						assign({ client: () => null, retryCount: () => 0 }),
					],
				},
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
			},
		},

		// Connected and ready for recording
		idle: {
			entry: [
				{ type: "emitConnectionState", params: { state: "idle" } },
				{ type: "emitReconnectResult", params: { success: true } },
				{ type: "logState", params: { state: "idle" } },
			],
			invoke: [
				{
					// Monitor for disconnection events
					src: "disconnectListener",
					input: ({ context }) => ({
						client: assertClient(context),
					}),
				},
				{
					// Handle provider change requests from main window
					src: "providerChangeListener",
					input: ({ context }) => ({
						client: assertClient(context),
					}),
				},
			],
			on: {
				DISCONNECTED: "retrying",
				COMMUNICATION_ERROR: {
					target: "retrying",
					actions: "cleanupClient",
				},
				START_RECORDING: {
					target: "startingRecording",
					actions: "enableClientMicrophoneForRecordingIfSupported",
				},
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

		// Preparing local mic capture and transport before active recording
		startingRecording: {
			entry: [
				{ type: "emitConnectionState", params: { state: "startingRecording" } },
				{ type: "logState", params: { state: "startingRecording" } },
			],
			invoke: {
				// Use disconnect listener - does NOT call connect()
				src: "disconnectListener",
				input: ({ context }) => ({
					client: assertClient(context),
				}),
			},
			on: {
				DISCONNECTED: {
					target: "retrying",
					actions: [
						"disableClientMicrophoneAfterRecordingIfSupported",
						"cleanupClient",
					],
				},
				COMMUNICATION_ERROR: {
					target: "retrying",
					actions: [
						"disableClientMicrophoneAfterRecordingIfSupported",
						"cleanupClient",
					],
				},
				START_RECORDING_READY: {
					target: "recording",
					actions: assign({ error: () => null }),
				},
				START_RECORDING_FAILED: {
					target: "idle",
					actions: [
						"disableClientMicrophoneAfterRecordingIfSupported",
						assign({ error: ({ event }) => event.error }),
					],
				},
				STOP_RECORDING: {
					target: "idle",
					actions: "disableClientMicrophoneAfterRecordingIfSupported",
				},
				SERVER_URL_CHANGED: {
					target: "initializing",
					actions: [
						"disableClientMicrophoneAfterRecordingIfSupported",
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
						"disableClientMicrophoneAfterRecordingIfSupported",
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
					client: assertClient(context),
				}),
			},
			on: {
				DISCONNECTED: {
					target: "retrying",
					actions: [
						"disableClientMicrophoneAfterRecordingIfSupported",
						"cleanupClient",
					],
				},
				COMMUNICATION_ERROR: {
					target: "retrying",
					actions: [
						"disableClientMicrophoneAfterRecordingIfSupported",
						"cleanupClient",
					],
				},
				STOP_RECORDING: {
					target: "processing",
					actions: "disableClientMicrophoneAfterRecordingIfSupported",
				},
				// Handle manual reconnect during recording
				RECONNECT: {
					target: "initializing",
					actions: [
						"disableClientMicrophoneAfterRecordingIfSupported",
						"cleanupClient",
						"emitReconnectStarted",
						assign({ client: () => null, retryCount: () => 0 }),
					],
				},
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
					client: assertClient(context),
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
				// Handle manual reconnect during processing
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
				retryDelay: "initializing",
			},
			on: {
				// Manual reconnect resets retry counter and retries immediately
				RECONNECT: {
					target: "initializing",
					actions: assign({ retryCount: () => 0 }),
				},
				// Server URL changed - immediately reconnect with new URL
				// No need for cleanupClient since it already runs on entry to retrying
				SERVER_URL_CHANGED: {
					target: "initializing",
					actions: assign({
						serverUrl: ({ event }) => event.serverUrl,
						retryCount: () => 0,
					}),
				},
			},
		},
	},
});

// Export types for consumers
export type ConnectionMachineActor = ActorRefFrom<typeof connectionMachine>;
export type ConnectionMachineStateValue = StateValueFrom<
	typeof connectionMachine
>;
export type { ConnectionContext, ConnectionEvents };
