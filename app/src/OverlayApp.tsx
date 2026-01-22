import { Loader } from "@mantine/core";
import { useResizeObserver, useTimeout } from "@mantine/hooks";
import {
	type BotLLMTextData,
	type PipecatClient,
	RTVIEvent,
} from "@pipecat-ai/client-js";
import {
	PipecatClientProvider,
	usePipecatClient,
	useRTVIClientEvent,
} from "@pipecat-ai/client-react";
import type { SmallWebRTCTransport } from "@pipecat-ai/small-webrtc-transport";
import { ThemeProvider, UserAudioComponent } from "@pipecat-ai/voice-ui-kit";
import { useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useDrag } from "@use-gesture/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { match } from "ts-pattern";
import { z } from "zod";
import Logo from "./assets/logo.svg?react";
import {
	ConnectionProvider,
	useConnectionClient,
	useConnectionSend,
	useConnectionState,
} from "./contexts/ConnectionContext";
import { useNativeAudioTrack } from "./hooks/useNativeAudioTrack";
import { useAddHistoryEntry, useSettings, useTypeText } from "./lib/queries";
import {
	matchSendResult,
	safeSendClientMessage,
} from "./lib/safeSendClientMessage";
import { tauriAPI } from "./lib/tauri";
import "./overlay-global.css";

const SERVER_RESPONSE_TIMEOUT_MS = 10_000;

// Server message schemas as a discriminated union for single-parse handling
const ServerMessageSchema = z.discriminatedUnion("type", [
	z.object({
		type: z.literal("recording-complete"),
		hasContent: z.boolean().optional(),
	}),
	// Provider switching still uses RTVI (requires frame injection)
	z.object({
		type: z.literal("config-updated"),
		setting: z.string(),
		value: z.unknown(),
		success: z.literal(true),
	}),
	z.object({
		type: z.literal("config-error"),
		setting: z.string(),
		error: z.string(),
	}),
]);

// Non-empty array type for type-safe batched sends
type NonEmptyArray<T> = [T, ...T[]];

// Discriminated union for type-safe config messages
// Only provider switching uses RTVI (requires frame injection into pipeline)
// Prompt sections and STT timeout now use HTTP API
type ConfigMessage =
	| { type: "set-stt-provider"; data: { provider: string } }
	| { type: "set-llm-provider"; data: { provider: string } };

function sendConfigMessages(
	client: PipecatClient,
	messages: NonEmptyArray<ConfigMessage>,
	onCommunicationError?: (error: string) => void,
) {
	for (const { type, data } of messages) {
		const result = safeSendClientMessage(
			client,
			type,
			data,
			onCommunicationError,
		);
		// If a message fails to send, stop sending further messages
		// The reconnection will handle re-syncing all settings
		const shouldContinue = matchSendResult(result, {
			onSuccess: () => true,
			onNotReady: () => false,
			onSendFailed: () => false,
		});
		if (!shouldContinue) {
			break;
		}
	}
}

// Hoisted static JSX for loading states (avoids recreation on every render)
const LoadingSpinner = (
	<div
		style={{
			width: 48,
			height: 48,
			display: "flex",
			alignItems: "center",
			justifyContent: "center",
		}}
	>
		<Loader size="sm" color="white" />
	</div>
);

const InitialLoadingSpinner = (
	<div
		className="flex items-center justify-center"
		style={{
			width: 48,
			height: 48,
			backgroundColor: "rgba(0, 0, 0, 0.9)",
			borderRadius: 12,
		}}
	>
		<Loader size="xs" color="white" />
	</div>
);

type DisplayState =
	| "disconnected"
	| "connecting"
	| "reconnecting"
	| "idle"
	| "recording"
	| "processing";

/**
 * Helper to convert XState state value to the ConnectionState string type
 * used by the UI and Tauri events.
 */
function getDisplayState(
	stateValue: string | Record<string, unknown>,
): DisplayState {
	// XState state values can be strings or objects (for nested states)
	const state =
		typeof stateValue === "string" ? stateValue : Object.keys(stateValue)[0];

	return match(state)
		.with("disconnected", () => "disconnected" as const)
		.with("initializing", "connecting", () => "connecting" as const)
		.with("retrying", () => "reconnecting" as const)
		.with("idle", () => "idle" as const)
		.with("recording", () => "recording" as const)
		.with("processing", () => "processing" as const)
		.otherwise(() => "disconnected" as const);
}

function RecordingControl() {
	const client = usePipecatClient();
	const queryClient = useQueryClient();
	const connectionState = useConnectionState();
	const send = useConnectionSend();

	// Convert XState state to display state
	const displayState = getDisplayState(connectionState);

	// Use Mantine's useResizeObserver hook
	const [containerRef, rect] = useResizeObserver();

	const hasWindowDragStartedRef = useRef(false);

	// State and refs for mic acquisition optimization
	const [isMicAcquiring, setIsMicAcquiring] = useState(false);
	const micPreparedRef = useRef(false);
	// Track the last mic device ID used for capture
	// undefined = never started, null = system default, string = specific device
	const lastMicIdRef = useRef<string | null | undefined>(undefined);

	// Native audio capture for low-latency mic acquisition
	// Bypasses browser's getUserMedia() which has ~300-400ms latency on macOS
	const {
		track: nativeAudioTrack,
		isReady: isNativeAudioReady,
		start: startNativeCapture,
		stop: stopNativeCapture,
	} = useNativeAudioTrack();

	const { data: settings } = useSettings();

	const streamedLlmResponseChunksRef = useRef("");

	// Track previous settings to detect actual changes (for syncing while connected)
	const prevSettingsRef = useRef(settings);

	const typeTextMutation = useTypeText();
	const addHistoryEntry = useAddHistoryEntry();

	const { start: startResponseTimeout, clear: clearResponseTimeout } =
		useTimeout(() => {
			if (displayState === "processing") {
				send({ type: "RESPONSE_RECEIVED" });
			}
		}, SERVER_RESPONSE_TIMEOUT_MS);

	// Clear response timeout when leaving processing state (reconnection, disconnection, etc.)
	// This prevents the timeout from firing after we've already transitioned away
	useEffect(() => {
		if (displayState !== "processing") {
			clearResponseTimeout();
		}
	}, [displayState, clearResponseTimeout]);

	// Auto-resize window to fit content using Mantine's useResizeObserver
	useEffect(() => {
		if (rect.width > 0 && rect.height > 0) {
			tauriAPI.resizeOverlay(Math.ceil(rect.width), Math.ceil(rect.height));
		}
	}, [rect.width, rect.height]);

	// Handle start/stop recording from hotkeys
	const onStartRecording = useCallback(async () => {
		// Always show loading indicator during mic acquisition and recording start
		// This ensures accurate UX feedback even when mic is pre-warmed
		setIsMicAcquiring(true);

		// Allow React to process the state update and show the loading indicator
		// before we start the async mic operations
		await new Promise((resolve) => setTimeout(resolve, 0));

		try {
			// Use native audio capture for low-latency mic acquisition
			if (isNativeAudioReady) {
				const deviceId = settings?.selected_mic_id ?? undefined;

				// Start native capture (skip if pre-warmed)
				if (!micPreparedRef.current) {
					await startNativeCapture(deviceId);
					lastMicIdRef.current = deviceId ?? null;
				}

				// Inject the native audio track into WebRTC
				if (client && nativeAudioTrack) {
					const transport = client.transport as SmallWebRTCTransport;
					const pc = (transport as unknown as { pc?: RTCPeerConnection }).pc;
					if (pc) {
						const audioSender = pc
							.getSenders()
							.find(
								(s) =>
									s.track?.kind === "audio" ||
									pc
										.getTransceivers()
										.some(
											(t) =>
												t.sender === s && t.receiver.track?.kind === "audio",
										),
							);

						if (audioSender) {
							await audioSender.replaceTrack(nativeAudioTrack);
						} else {
							const stream = new MediaStream([nativeAudioTrack]);
							pc.addTrack(nativeAudioTrack, stream);
						}
					}
				}
			}

			// Reset prepared state for next recording
			micPreparedRef.current = false;

			// Enable mic and transition to recording state
			if (client) {
				try {
					client.enableMic(true);
				} catch (error) {
					console.warn("[Recording] Failed to enable mic:", error);
					return;
				}
				send({ type: "START_RECORDING" });

				// Signal server to start turn management
				// This is required for server-side buffer management and turn detection
				// Use safe send to detect communication failures and trigger reconnection
				safeSendClientMessage(client, "start-recording", {}, (error) =>
					send({ type: "COMMUNICATION_ERROR", error }),
				);
			}
		} catch (error) {
			console.warn("[Recording] Failed to start recording:", error);
		} finally {
			setIsMicAcquiring(false);
		}
	}, [
		client,
		settings?.selected_mic_id,
		isNativeAudioReady,
		nativeAudioTrack,
		startNativeCapture,
		send,
	]);

	const onStopRecording = useCallback(() => {
		// Stop native audio capture and reset state so next recording starts fresh
		stopNativeCapture();
		lastMicIdRef.current = undefined;

		// Always disable mic and detach track, regardless of displayState
		// This ensures the mic indicator goes away even if state changed
		if (client) {
			// Disable mic to release any browser getUserMedia stream
			try {
				client.enableMic(false);
			} catch (error) {
				console.warn("[Recording] Failed to disable mic:", error);
			}

			// Detach the native audio track from WebRTC sender to stop transmitting
			// (enableMic only affects the client's internal track, not our injected native track)
			try {
				const transport = client.transport as SmallWebRTCTransport;
				const pc = (transport as unknown as { pc?: RTCPeerConnection }).pc;
				if (pc) {
					const audioSender = pc
						.getSenders()
						.find((s) => s.track?.kind === "audio");
					if (audioSender) {
						audioSender.replaceTrack(null);
					}
				}
			} catch (error) {
				console.warn("[Recording] Failed to detach audio track:", error);
			}

			// Stop the audio track immediately to release the microphone
			try {
				const tracks = client.tracks();
				if (tracks?.local?.audio) {
					tracks.local.audio.stop();
				}
			} catch (error) {
				console.warn("[Recording] Failed to stop audio track:", error);
			}
		}

		// Only do state transitions and server signaling if we were actually recording
		if (client && displayState === "recording") {
			// Transition to processing state and start timeout
			send({ type: "STOP_RECORDING" });
			startResponseTimeout();

			// Signal server to process the recorded audio
			// This is required for server-side turn completion
			// Use safe send to detect communication failures and trigger reconnection
			safeSendClientMessage(client, "stop-recording", {}, (error) =>
				send({ type: "COMMUNICATION_ERROR", error }),
			);
		}
	}, [client, displayState, stopNativeCapture, send, startResponseTimeout]);

	useEffect(() => {
		let isCancelled = false;
		let unlistenStart: (() => void) | undefined;
		let unlistenStop: (() => void) | undefined;

		const setup = async () => {
			const [startUnlisten, stopUnlisten] = await Promise.all([
				tauriAPI.onStartRecording(onStartRecording),
				tauriAPI.onStopRecording(onStopRecording),
			]);

			// If cancelled before setup completed, clean up immediately
			if (isCancelled) {
				startUnlisten();
				stopUnlisten();
				return;
			}

			unlistenStart = startUnlisten;
			unlistenStop = stopUnlisten;
		};

		setup();

		return () => {
			isCancelled = true;
			unlistenStart?.();
			unlistenStop?.();
		};
	}, [onStartRecording, onStopRecording]);

	// Listen for prepare-recording event (toggle key press) to pre-warm microphone
	// This reduces perceived latency by acquiring the mic while user holds the key
	useEffect(() => {
		let unlisten: (() => void) | undefined;

		const setup = async () => {
			unlisten = await tauriAPI.onPrepareRecording(async () => {
				// Only prepare if we're idle and not already prepared
				if (
					!micPreparedRef.current &&
					displayState === "idle" &&
					isNativeAudioReady
				) {
					const deviceId = settings?.selected_mic_id ?? undefined;
					setIsMicAcquiring(true);
					try {
						await startNativeCapture(deviceId);
						lastMicIdRef.current = deviceId ?? null;
					} catch (error) {
						console.warn("[Recording] Failed to pre-warm mic:", error);
					}
					setIsMicAcquiring(false);
					micPreparedRef.current = true;
				}
			});
		};

		setup();

		return () => {
			unlisten?.();
		};
	}, [
		settings?.selected_mic_id,
		displayState,
		isNativeAudioReady,
		startNativeCapture,
	]);

	// Listen for settings changes from main window and invalidate cache to trigger sync
	useEffect(() => {
		let unlisten: (() => void) | undefined;

		const setup = async () => {
			unlisten = await tauriAPI.onSettingsChanged(() => {
				// Invalidate settings query to trigger refetch from Tauri Store
				// The settings sync useEffect will then detect the change and sync to server
				queryClient.invalidateQueries({ queryKey: ["settings"] });
			});
		};

		setup();

		return () => {
			unlisten?.();
		};
	}, [queryClient]);

	// Listen for disconnect request from Rust (triggered on app quit)
	useEffect(() => {
		let unlisten: (() => void) | undefined;

		const setup = async () => {
			unlisten = await listen("request-disconnect", async () => {
				console.log("[Pipecat] Received disconnect request from Rust");
				if (client) {
					try {
						await client.disconnect();
						console.log("[Pipecat] Disconnected gracefully");
					} catch (error) {
						console.error("[Pipecat] Disconnect error:", error);
					}
				}
			});
		};

		setup();

		return () => {
			unlisten?.();
		};
	}, [client]);

	// Cleanup on window close/beforeunload
	useEffect(() => {
		const handleBeforeUnload = () => {
			client?.disconnect();
		};
		window.addEventListener("beforeunload", handleBeforeUnload);
		return () => window.removeEventListener("beforeunload", handleBeforeUnload);
	}, [client]);

	// Build config messages from current settings (used for initial sync and change detection)
	// Only provider switching uses RTVI - prompt sections and STT timeout use HTTP API
	const buildConfigMessages = useCallback(
		(
			currentSettings: typeof settings,
			prevSettings?: typeof settings,
		): ConfigMessage[] => {
			const messages: ConfigMessage[] = [];

			const hasChanged = (key: keyof NonNullable<typeof settings>) => {
				const current = currentSettings?.[key];
				const prev = prevSettings?.[key];
				if (current == null) return false;
				if (prevSettings === undefined) return true; // Initial sync
				return current !== prev;
			};

			if (hasChanged("stt_provider")) {
				messages.push({
					type: "set-stt-provider",
					data: { provider: currentSettings?.stt_provider as string },
				});
			}
			if (hasChanged("llm_provider")) {
				messages.push({
					type: "set-llm-provider",
					data: { provider: currentSettings?.llm_provider as string },
				});
			}

			return messages;
		},
		[],
	);

	// Track if initial settings sync has been done for this connection
	const hasInitialSyncRef = useRef(false);

	// Sync provider settings when they change OR on initial connection (state transitions to 'idle')
	// Provider switching uses RTVI (requires frame injection into pipeline)
	useEffect(() => {
		const prevSettings = prevSettingsRef.current;
		prevSettingsRef.current = settings;

		// Only sync if connected (idle state)
		if (!client || displayState !== "idle") {
			// Reset initial sync flag and notify Rust when disconnected
			if (
				displayState === "disconnected" ||
				displayState === "connecting" ||
				displayState === "reconnecting"
			) {
				if (hasInitialSyncRef.current) {
					hasInitialSyncRef.current = false;
					tauriAPI.setServerDisconnected();
				}
			}
			return;
		}

		// Initial sync after connection
		// Now that we properly wait for "ready" state before transitioning to idle,
		// we can send messages immediately without waiting
		if (!hasInitialSyncRef.current) {
			hasInitialSyncRef.current = true;

			// Notify Rust of connection so it can sync settings via HTTP
			const notifyRust = async () => {
				const serverUrl = await tauriAPI.getServerUrl();
				const clientUUID = await tauriAPI.getClientUUID();
				if (serverUrl && clientUUID) {
					await tauriAPI.setServerConnected(serverUrl, clientUUID);
				}
			};
			notifyRust();

			// Error handler for communication failures during sync
			const handleCommunicationError = (error: string) =>
				send({ type: "COMMUNICATION_ERROR", error });

			// Send provider settings via RTVI (requires frame injection)
			const messages = buildConfigMessages(settings);
			if (messages.length > 0) {
				sendConfigMessages(
					client,
					messages as NonEmptyArray<ConfigMessage>,
					handleCommunicationError,
				);
			}
			return;
		}

		// Runtime settings change - only send if settings actually changed
		if (prevSettings === settings) return;

		// Provider changes via RTVI (only provider switching uses RTVI now)
		const messages = buildConfigMessages(settings, prevSettings);
		if (messages.length > 0) {
			sendConfigMessages(
				client,
				messages as NonEmptyArray<ConfigMessage>,
				(error) => send({ type: "COMMUNICATION_ERROR", error }),
			);
		}
	}, [client, displayState, settings, buildConfigMessages, send]);

	// LLM text streaming handlers (using official RTVI protocol via RTVIObserver)
	useRTVIClientEvent(
		RTVIEvent.BotLlmStarted,
		useCallback(() => {
			// Reset accumulator when LLM starts generating
			streamedLlmResponseChunksRef.current = "";
		}, []),
	);

	useRTVIClientEvent(
		RTVIEvent.BotLlmText,
		useCallback((data: BotLLMTextData) => {
			// Accumulate text chunks from LLM
			streamedLlmResponseChunksRef.current += data.text;
		}, []),
	);

	useRTVIClientEvent(
		RTVIEvent.BotLlmStopped,
		useCallback(async () => {
			clearResponseTimeout();
			const text = streamedLlmResponseChunksRef.current.trim();
			streamedLlmResponseChunksRef.current = "";

			if (text) {
				console.debug("[Pipecat] LLM response:", text);
				try {
					await typeTextMutation.mutateAsync(text);
				} catch (error) {
					console.error("[Pipecat] Failed to type text:", error);
				}
				addHistoryEntry.mutate(text);
			}
			send({ type: "RESPONSE_RECEIVED" });
		}, [clearResponseTimeout, typeTextMutation, addHistoryEntry, send]),
	);

	// Server message handler (for custom messages: config-updated, recording-complete, etc.)
	useRTVIClientEvent(
		RTVIEvent.ServerMessage,
		useCallback(
			(message: unknown) => {
				const result = ServerMessageSchema.safeParse(message);
				if (!result.success) return;

				match(result.data)
					.with({ type: "recording-complete" }, () => {
						clearResponseTimeout();
						send({ type: "RESPONSE_RECEIVED" });
					})
					.with({ type: "config-updated" }, ({ setting, value }) => {
						// Only provider switching responses come via RTVI now
						tauriAPI.emitConfigResponse({
							type: "config-updated",
							setting,
							value,
						});
					})
					.with({ type: "config-error" }, ({ setting, error }) => {
						// Only provider switching errors come via RTVI now
						tauriAPI.emitConfigResponse({
							type: "config-error",
							setting,
							error,
						});
					})
					.exhaustive();
			},
			[clearResponseTimeout, send],
		),
	);

	useRTVIClientEvent(
		RTVIEvent.Error,
		useCallback(
			(error: unknown) => {
				console.error("[Pipecat] Error:", error);

				// Check if this is a fatal error that requires reconnection
				const errorData = error as {
					data?: { message?: string; fatal?: boolean };
				};
				if (errorData?.data?.fatal) {
					console.warn(
						"[Pipecat] Fatal error detected, triggering reconnection",
					);
					send({
						type: "COMMUNICATION_ERROR",
						error: errorData.data.message ?? "Fatal error",
					});
				}
			},
			[send],
		),
	);

	useRTVIClientEvent(
		RTVIEvent.DeviceError,
		useCallback((error: unknown) => {
			console.error("[Pipecat] Device error:", error);
		}, []),
	);

	// Click handler (toggle mode)
	const handleClick = useCallback(() => {
		if (displayState === "recording") {
			onStopRecording();
		} else if (displayState === "idle") {
			onStartRecording();
		}
	}, [displayState, onStartRecording, onStopRecording]);

	// Drag handler using @use-gesture/react
	// Handles unfocused window dragging (data-tauri-drag-region doesn't work on unfocused windows)
	const bindDrag = useDrag(
		({ movement: [mx, my], first, last, memo }) => {
			if (first) {
				hasWindowDragStartedRef.current = false;
				return false; // memo = false (hasn't started dragging)
			}

			const distance = Math.sqrt(mx * mx + my * my);
			const DRAG_THRESHOLD = 5;

			// Start dragging once threshold is exceeded
			if (!memo && distance > DRAG_THRESHOLD) {
				hasWindowDragStartedRef.current = true;
				tauriAPI.startDragging();
				return true; // memo = true (dragging started)
			}

			if (last) {
				hasWindowDragStartedRef.current = false;
			}

			return memo;
		},
		{ filterTaps: true },
	);

	return (
		<div
			ref={containerRef}
			role="application"
			{...bindDrag()}
			style={{
				width: "fit-content",
				height: "fit-content",
				backgroundColor: "rgba(0, 0, 0, 0.9)",
				borderRadius: 12,
				border: "1px solid rgba(128, 128, 128, 0.9)",
				padding: 2,
				cursor: "grab",
				userSelect: "none",
				touchAction: "none",
			}}
		>
			{displayState === "processing" ||
			displayState === "disconnected" ||
			displayState === "connecting" ||
			displayState === "reconnecting" ||
			isMicAcquiring ? (
				LoadingSpinner
			) : (
				<UserAudioComponent
					onClick={handleClick}
					isMicEnabled={displayState === "recording"}
					noIcon={true}
					noDevicePicker={true}
					noVisualizer={displayState !== "recording"}
					visualizerProps={{
						barColor: "#eeeeee",
						backgroundColor: "#000000",
					}}
					classNames={{
						button: "bg-black text-white hover:bg-gray-900",
					}}
				>
					{displayState !== "recording" && <Logo className="size-5" />}
				</UserAudioComponent>
			)}
		</div>
	);
}

/**
 * Wrapper component that waits for the client to be available
 * before rendering the recording control.
 */
function RecordingControlWithClient() {
	const client = useConnectionClient();

	if (!client) {
		return InitialLoadingSpinner;
	}

	return (
		<PipecatClientProvider client={client}>
			<RecordingControl />
		</PipecatClientProvider>
	);
}

export default function OverlayApp() {
	return (
		<ConnectionProvider>
			<ThemeProvider>
				<RecordingControlWithClient />
			</ThemeProvider>
		</ConnectionProvider>
	);
}
