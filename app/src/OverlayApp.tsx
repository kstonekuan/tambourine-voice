import { Loader } from "@mantine/core";
import { useResizeObserver, useTimeout } from "@mantine/hooks";
import {
	type BotLLMTextData,
	RTVIEvent,
	type TranscriptData,
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
import { AlertCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { match } from "ts-pattern";
import { z } from "zod";
import Logo from "./assets/logo.svg?react";
import {
	ConnectionProvider,
	useConnectionActor,
	useConnectionClient,
	useConnectionSend,
	useConnectionState,
} from "./contexts/ConnectionContext";
import { useNativeAudioTrack } from "./hooks/useNativeAudioTrack";
import type { ActiveAppContextSnapshot } from "./lib/activeAppContext";
import { useAddHistoryEntry, useSettings, useTypeText } from "./lib/queries";
import { safeSendClientMessage } from "./lib/safeSendClientMessage";
import { tauriAPI } from "./lib/tauri";
import type { ConnectionMachineStateValue } from "./machines/connectionMachine";
import "./overlay-global.css";

const SERVER_RESPONSE_TIMEOUT_MS = 10_000;
const NATIVE_AUDIO_FIRST_FRAME_READY_TIMEOUT_MS = 500;
const PIPECAT_LOCAL_PARTICIPANT = {
	id: "local",
	name: "local",
	local: true,
} as const;

// Server message schemas as a discriminated union for single-parse handling
const KnownServerMessageSchema = z.discriminatedUnion("type", [
	z.object({
		type: z.literal("recording-complete"),
		hasContent: z.boolean().optional(),
	}),
	// Raw transcription (LLM bypassed) - sent when LLM formatting is disabled
	z.object({
		type: z.literal("raw-transcription"),
		text: z.string(),
	}),
	// Provider switching uses RTVI (requires frame injection into pipeline)
	// z.enum() validates known settings; unknown settings become UnknownServerMessage
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

type KnownServerMessage = z.infer<typeof KnownServerMessageSchema>;

/**
 * Unknown server message type (forward compatibility).
 *
 * Preserves the raw message data for debugging, similar to
 * UnknownClientMessage pattern on the server side.
 */
type UnknownServerMessage = {
	type: "unknown";
	originalType: string;
	raw: unknown;
};

type ServerMessage = KnownServerMessage | UnknownServerMessage;

/**
 * Parse server message with forward compatibility.
 *
 * Returns UnknownServerMessage for unknown types (never null).
 * This allows exhaustive pattern matching while preserving raw data
 * for debugging purposes.
 */
function parseServerMessage(raw: unknown): ServerMessage {
	const result = KnownServerMessageSchema.safeParse(raw);
	if (!result.success) {
		const originalType = (raw as { type?: string })?.type ?? "";
		// Log at warn level so it's visible by default in devtools
		console.warn(
			"[Pipecat] Failed to parse server message:",
			originalType,
			"\nRaw:",
			raw,
			"\nZod errors:",
			result.error.issues,
		);
		return { type: "unknown", originalType, raw };
	}
	return result.data;
}

// Schema for validating RTVI error payloads
const RTVIErrorSchema = z.object({
	data: z
		.object({
			message: z.string().optional(),
			fatal: z.boolean().optional(),
		})
		.optional(),
});

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

// Simple error indicator - detailed error goes to main window toast
// Click to dismiss and start recording
const ErrorDisplay = ({
	onDismiss,
	onStartRecording,
}: {
	onDismiss: () => void;
	onStartRecording?: () => void;
}) => (
	<button
		type="button"
		onClick={() => {
			onDismiss();
			onStartRecording?.();
		}}
		style={{
			minWidth: 48,
			width: "fit-content",
			minHeight: 48,
			display: "flex",
			flexDirection: "column",
			alignItems: "center",
			justifyContent: "center",
			gap: 4,
			cursor: "pointer",
			padding: "4px 8px",
			background: "none",
			border: "none",
		}}
	>
		<AlertCircle size={20} color="#f87171" />
		<span
			style={{
				fontSize: 9,
				color: "#fca5a5",
				textAlign: "center",
				lineHeight: 1.2,
				whiteSpace: "nowrap",
			}}
		>
			Try again
		</span>
	</button>
);

type DisplayState =
	| "disconnected"
	| "connecting"
	| "reconnecting"
	| "idle"
	| "startingRecording"
	| "recording"
	| "processing";

function getDisplayState(
	stateValue: ConnectionMachineStateValue,
): DisplayState {
	return match(stateValue)
		.with("disconnected", () => "disconnected" as const)
		.with("initializing", "connecting", "syncing", () => "connecting" as const)
		.with("retrying", () => "reconnecting" as const)
		.with("idle", () => "idle" as const)
		.with("startingRecording", () => "startingRecording" as const)
		.with("recording", () => "recording" as const)
		.with("processing", () => "processing" as const)
		.exhaustive();
}

function getPeerConnectionAudioSender(
	peerConnection: RTCPeerConnection,
): RTCRtpSender | undefined {
	return peerConnection
		.getSenders()
		.find(
			(sender) =>
				sender.track?.kind === "audio" ||
				peerConnection
					.getTransceivers()
					.some(
						(transceiver) =>
							transceiver.sender === sender &&
							transceiver.receiver.track?.kind === "audio",
					),
		);
}

function RecordingControl() {
	const connectionActor = useConnectionActor();
	const client = usePipecatClient();
	const queryClient = useQueryClient();
	const connectionState = useConnectionState();
	const send = useConnectionSend();
	const displayState = getDisplayState(connectionState);

	// Use Mantine's useResizeObserver hook
	const [containerRef, rect] = useResizeObserver();

	const hasWindowDragStartedRef = useRef(false);

	// State and refs for mic acquisition optimization
	const [isMicAcquiring, setIsMicAcquiring] = useState(false);
	const micPreparedRef = useRef(false);
	const latestActiveAppContextRef = useRef<ActiveAppContextSnapshot | null>(
		null,
	);
	const activeAppContextSentForCurrentRecordingRef =
		useRef<ActiveAppContextSnapshot | null>(null);
	// Track the last mic device ID used for capture
	// undefined = never started, null = system default, string = specific device
	const lastMicIdRef = useRef<string | null | undefined>(undefined);

	// Native audio capture for low-latency mic acquisition
	// Bypasses browser's getUserMedia() which has ~300-400ms latency on macOS
	const {
		track: nativeAudioTrack,
		getCurrentTrack: getCurrentNativeAudioTrack,
		waitUntilReady: waitUntilNativeAudioReady,
		start: startNativeCapture,
		stop: stopNativeCapture,
	} = useNativeAudioTrack();

	const { data: settings } = useSettings();

	const streamedLlmResponseChunksRef = useRef("");
	const rawTranscriptionRef = useRef("");

	const typeTextMutation = useTypeText();
	const addHistoryEntry = useAddHistoryEntry();

	// Error display state (persists until user records again)
	const [showError, setShowError] = useState(false);

	const { start: startResponseTimeout, clear: clearResponseTimeout } =
		useTimeout(() => {
			if (displayState === "processing") {
				// Show simple error in overlay
				setShowError(true);

				// Send detailed error to main window
				tauriAPI.emitLLMError({
					message: "Response timed out - the server took too long to respond",
					fatal: false,
				});

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
	const onStartRecording = useCallback(() => {
		send({ type: "START_RECORDING" });
	}, [send]);

	useEffect(() => {
		if (displayState !== "startingRecording") {
			return;
		}

		let shouldIgnoreStartResults = false;
		let didTransitionToRecording = false;
		let hasTornDownCaptureForStartAttempt = false;

		const tearDownCaptureForAbortedStartAttempt = () => {
			if (hasTornDownCaptureForStartAttempt) {
				return;
			}

			stopNativeCapture();
			lastMicIdRef.current = undefined;
			micPreparedRef.current = false;
			hasTornDownCaptureForStartAttempt = true;
		};

		const startRecordingFromMachineState = async () => {
			// Clear error state when starting recording
			setShowError(false);

			// Reset accumulators for new recording
			// Important: rawTranscriptionRef is reset here (not on BotLlmStarted)
			// because UserTranscript events arrive DURING recording, before LLM processes
			streamedLlmResponseChunksRef.current = "";
			rawTranscriptionRef.current = "";
			activeAppContextSentForCurrentRecordingRef.current = null;

			// Always show loading indicator during mic acquisition and recording start
			// This ensures accurate UX feedback even when mic is pre-warmed
			setIsMicAcquiring(true);

			// Allow React to process the state update and show the loading indicator
			// before we start the async mic operations
			await new Promise((resolve) => setTimeout(resolve, 0));

			try {
				if (!client) {
					send({
						type: "START_RECORDING_FAILED",
						error: "Pipecat client is unavailable while starting recording",
					});
					return;
				}

				await waitUntilNativeAudioReady(
					NATIVE_AUDIO_FIRST_FRAME_READY_TIMEOUT_MS,
				);
				if (shouldIgnoreStartResults) {
					return;
				}

				const selectedMicDeviceId = settings?.selected_mic_id ?? undefined;
				if (!micPreparedRef.current) {
					const startNativeCaptureResult = await startNativeCapture({
						deviceId: selectedMicDeviceId,
						waitForFirstAudioFrameMs: NATIVE_AUDIO_FIRST_FRAME_READY_TIMEOUT_MS,
					});
					if (shouldIgnoreStartResults) {
						return;
					}

					lastMicIdRef.current = selectedMicDeviceId ?? null;

					if (!startNativeCaptureResult.firstFrameReceived) {
						console.warn(
							"[Recording] Native mic did not produce audio frames before timeout",
						);
						setShowError(true);
						tearDownCaptureForAbortedStartAttempt();
						send({
							type: "START_RECORDING_FAILED",
							error: "Native microphone timed out before first audio frame",
						});
						return;
					}
				}

				const nativeAudioTrackForRecording = getCurrentNativeAudioTrack();
				if (!nativeAudioTrackForRecording) {
					throw new Error("Native audio track is unavailable");
				}

				const transport = client.transport as SmallWebRTCTransport;
				const peerConnection = (
					transport as unknown as { pc?: RTCPeerConnection }
				).pc;
				if (!peerConnection) {
					throw new Error("WebRTC peer connection is unavailable");
				}

				const audioSender = getPeerConnectionAudioSender(peerConnection);
				if (audioSender) {
					await audioSender.replaceTrack(nativeAudioTrackForRecording);
				} else {
					const nativeAudioTrackStream = new MediaStream([
						nativeAudioTrackForRecording,
					]);
					peerConnection.addTrack(
						nativeAudioTrackForRecording,
						nativeAudioTrackStream,
					);
				}
				if (shouldIgnoreStartResults) {
					return;
				}

				// Reset prepared state for next recording
				micPreparedRef.current = false;

				// Signal server to start turn management
				// LLM formatting is now controlled globally via the config API
				// Use safe send to detect communication failures and trigger reconnection
				const activeAppContextSentForCurrentRecording =
					settings?.send_active_app_context_enabled === true
						? latestActiveAppContextRef.current
						: null;
				activeAppContextSentForCurrentRecordingRef.current =
					activeAppContextSentForCurrentRecording;
				const startRecordingData = activeAppContextSentForCurrentRecording
					? { active_app_context: activeAppContextSentForCurrentRecording }
					: {};
				console.debug(
					"[Active App Context] Sending start-recording payload:",
					startRecordingData,
				);
				safeSendClientMessage(
					client,
					"start-recording",
					startRecordingData,
					(error) => send({ type: "COMMUNICATION_ERROR", error }),
				);

				send({ type: "START_RECORDING_READY" });
				didTransitionToRecording =
					connectionActor.getSnapshot().value === "recording";

				// Emit after state transition tick so VoiceVisualizer has subscribed.
				setTimeout(() => {
					const isStillRecordingState =
						connectionActor.getSnapshot().value === "recording";
					if (
						shouldIgnoreStartResults ||
						!didTransitionToRecording ||
						!isStillRecordingState
					) {
						return;
					}

					client.emit(
						RTVIEvent.TrackStarted,
						nativeAudioTrackForRecording,
						PIPECAT_LOCAL_PARTICIPANT,
					);
				}, 0);
			} catch (error) {
				if (shouldIgnoreStartResults) {
					return;
				}

				console.warn("[Recording] Failed to start recording:", error);
				tearDownCaptureForAbortedStartAttempt();
				setShowError(true);
				send({
					type: "START_RECORDING_FAILED",
					error: error instanceof Error ? error.message : String(error),
				});
			} finally {
				if (!shouldIgnoreStartResults) {
					setIsMicAcquiring(false);
				}
			}
		};

		startRecordingFromMachineState();

		return () => {
			const didAbortStartAttemptBeforeRecording = !didTransitionToRecording;
			shouldIgnoreStartResults = didAbortStartAttemptBeforeRecording;
			setIsMicAcquiring(false);

			if (didAbortStartAttemptBeforeRecording) {
				tearDownCaptureForAbortedStartAttempt();
			}
		};
	}, [
		client,
		displayState,
		settings?.selected_mic_id,
		settings?.send_active_app_context_enabled,
		connectionActor,
		getCurrentNativeAudioTrack,
		send,
		startNativeCapture,
		stopNativeCapture,
		waitUntilNativeAudioReady,
	]);

	const onStopRecording = useCallback(() => {
		if (displayState === "processing") {
			return;
		}

		setIsMicAcquiring(false);
		micPreparedRef.current = false;

		// Stop native audio capture and reset state so next recording starts fresh
		stopNativeCapture();
		lastMicIdRef.current = undefined;

		// Always detach track, regardless of displayState
		// This ensures the mic indicator goes away even if state changed
		if (client) {
			// Detach the native audio track from WebRTC sender to stop transmitting
			try {
				const transport = client.transport as SmallWebRTCTransport;
				const pc = (transport as unknown as { pc?: RTCPeerConnection }).pc;
				if (pc) {
					const audioSender = getPeerConnectionAudioSender(pc);
					if (audioSender) {
						audioSender.replaceTrack(null);
					}
				}
				if (nativeAudioTrack) {
					client.emit(
						RTVIEvent.TrackStopped,
						nativeAudioTrack,
						PIPECAT_LOCAL_PARTICIPANT,
					);
				}
			} catch (error) {
				console.warn("[Recording] Failed to detach audio track:", error);
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
		} else {
			if (displayState === "startingRecording") {
				send({ type: "STOP_RECORDING" });
			}
			activeAppContextSentForCurrentRecordingRef.current = null;
		}
	}, [
		client,
		displayState,
		nativeAudioTrack,
		stopNativeCapture,
		send,
		startResponseTimeout,
	]);

	useEffect(() => {
		if (displayState !== "recording" && displayState !== "processing") {
			activeAppContextSentForCurrentRecordingRef.current = null;
		}
	}, [displayState]);

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
				if (!micPreparedRef.current && displayState === "idle") {
					const selectedMicDeviceId = settings?.selected_mic_id ?? undefined;
					setIsMicAcquiring(true);
					try {
						await waitUntilNativeAudioReady(
							NATIVE_AUDIO_FIRST_FRAME_READY_TIMEOUT_MS,
						);

						const startNativeCaptureResult = await startNativeCapture({
							deviceId: selectedMicDeviceId,
							waitForFirstAudioFrameMs:
								NATIVE_AUDIO_FIRST_FRAME_READY_TIMEOUT_MS,
						});

						if (!startNativeCaptureResult.firstFrameReceived) {
							console.warn(
								"[Recording] Native mic pre-warm timed out before first frame",
							);
							stopNativeCapture();
							lastMicIdRef.current = undefined;
							micPreparedRef.current = false;
							return;
						}

						lastMicIdRef.current = selectedMicDeviceId ?? null;
						micPreparedRef.current = true;
					} catch (error) {
						console.warn("[Recording] Failed to pre-warm mic:", error);
						stopNativeCapture();
						lastMicIdRef.current = undefined;
						micPreparedRef.current = false;
					} finally {
						setIsMicAcquiring(false);
					}
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
		waitUntilNativeAudioReady,
		startNativeCapture,
		stopNativeCapture,
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

	// Listen for active app context updates from Rust
	useEffect(() => {
		let unlisten: (() => void) | undefined;
		let shouldIgnoreSetupResults = false;

		const setup = async () => {
			unlisten = await tauriAPI.onActiveAppContextChanged((payload) => {
				latestActiveAppContextRef.current = payload;
			});

			try {
				const seededActiveAppContextSnapshot =
					await tauriAPI.activeAppGetCurrentContext();
				if (shouldIgnoreSetupResults) {
					return;
				}

				// Seed startup active app context only when no live event has populated it yet.
				// Keep startup behavior simple and best-effort without recency comparisons.
				if (!latestActiveAppContextRef.current) {
					latestActiveAppContextRef.current = seededActiveAppContextSnapshot;
				}
			} catch (error) {
				console.warn(
					"[Active App Context] Failed to fetch startup focus snapshot:",
					error,
				);
			}
		};

		setup();

		return () => {
			shouldIgnoreSetupResults = true;
			unlisten?.();
		};
	}, []);

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

	// Track if initial settings sync has been done for this connection
	const hasInitialSyncRef = useRef(false);

	// Notify Rust backend of connection/disconnection state changes
	// Provider settings are synced via initialConfigSync actor in the connection machine
	// Runtime provider changes are handled via onProviderChangeRequest event (pessimistic updates)
	useEffect(() => {
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

		// Notify Rust of connection so it can sync settings via HTTP
		// (prompt sections and STT timeout use the HTTP API, not RTVI)
		if (!hasInitialSyncRef.current) {
			hasInitialSyncRef.current = true;

			const notifyRust = async () => {
				const serverUrl = await tauriAPI.getServerUrl();
				const clientUUID = await tauriAPI.getClientUUID();
				if (serverUrl && clientUUID) {
					await tauriAPI.setServerConnected(serverUrl, clientUUID);
				}
			};
			notifyRust();
		}
	}, [client, displayState]);

	// Listen to native UserTranscript event for raw transcription
	// RTVIObserver emits these automatically as user speaks
	useRTVIClientEvent(
		RTVIEvent.UserTranscript,
		useCallback((data: TranscriptData) => {
			// Accumulate final transcriptions (ignore partials to avoid duplicates)
			if (data.final) {
				rawTranscriptionRef.current +=
					(rawTranscriptionRef.current ? " " : "") + data.text;
			}
		}, []),
	);

	// LLM text streaming handlers (using official RTVI protocol via RTVIObserver)
	useRTVIClientEvent(
		RTVIEvent.BotLlmStarted,
		useCallback(() => {
			// Reset LLM accumulator when LLM starts generating
			// Note: rawTranscriptionRef is reset on recording start, not here
			// (transcripts arrive DURING recording, before LLM processes)
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
			const rawText = rawTranscriptionRef.current.trim();
			const activeAppContextSentForCurrentRecording =
				activeAppContextSentForCurrentRecordingRef.current;
			streamedLlmResponseChunksRef.current = "";
			rawTranscriptionRef.current = "";

			if (text) {
				console.debug("[Pipecat] LLM response:", text);
				console.debug("[Pipecat] Raw transcription:", rawText);
				try {
					await typeTextMutation.mutateAsync(text);
				} catch (error) {
					console.error("[Pipecat] Failed to type text:", error);
				}
				addHistoryEntry.mutate({
					text,
					rawText,
					activeAppContext: activeAppContextSentForCurrentRecording,
				});
			}
			activeAppContextSentForCurrentRecordingRef.current = null;
			send({ type: "RESPONSE_RECEIVED" });
		}, [clearResponseTimeout, typeTextMutation, addHistoryEntry, send]),
	);

	// Server message handler (for custom messages: config-updated, recording-complete, raw-transcription, etc.)
	useRTVIClientEvent(
		RTVIEvent.ServerMessage,
		useCallback(
			async (message: unknown) => {
				// Use forward-compatible parser (never returns null)
				const parsed = parseServerMessage(message);

				match(parsed)
					.with({ type: "recording-complete" }, () => {
						clearResponseTimeout();
						activeAppContextSentForCurrentRecordingRef.current = null;
						send({ type: "RESPONSE_RECEIVED" });
					})
					.with({ type: "raw-transcription" }, async ({ text }) => {
						// Raw transcription received (LLM bypassed)
						clearResponseTimeout();
						const activeAppContextSentForCurrentRecording =
							activeAppContextSentForCurrentRecordingRef.current;
						const trimmedText = text.trim();

						if (trimmedText) {
							console.debug(
								"[Pipecat] Raw transcription (LLM bypassed):",
								trimmedText,
							);
							try {
								await typeTextMutation.mutateAsync(trimmedText);
							} catch (error) {
								console.error("[Pipecat] Failed to type text:", error);
							}
							// For raw transcription, text and rawText are the same
							addHistoryEntry.mutate({
								text: trimmedText,
								rawText: trimmedText,
								activeAppContext: activeAppContextSentForCurrentRecording,
							});
						}
						activeAppContextSentForCurrentRecordingRef.current = null;
						send({ type: "RESPONSE_RECEIVED" });
					})
					.with({ type: "config-updated" }, ({ setting, value }) => {
						tauriAPI.emitConfigResponse({
							type: "config-updated",
							setting,
							value,
						});
					})
					.with({ type: "config-error" }, ({ setting, error }) => {
						console.error("[Pipecat] Config error:", setting, error);
						tauriAPI.emitConfigResponse({
							type: "config-error",
							setting,
							error,
						});
					})
					.with({ type: "unknown" }, () => {
						// Already logged at warn level in parseServerMessage
					})
					.exhaustive();
			},
			[clearResponseTimeout, send, typeTextMutation, addHistoryEntry],
		),
	);

	useRTVIClientEvent(
		RTVIEvent.Error,
		useCallback(
			(error: unknown) => {
				console.error("[Pipecat] Error:", error);

				const parsed = RTVIErrorSchema.safeParse(error);
				if (parsed.success) {
					const errorData = parsed.data.data;
					const message = errorData?.message ?? "Unknown error";

					// Show simple "Try again" in overlay
					setShowError(true);

					// Send detailed error to main window toast
					tauriAPI.emitLLMError({
						message,
						fatal: errorData?.fatal ?? false,
					});

					// Return to idle if in processing state (error means no content coming)
					if (displayState === "processing") {
						clearResponseTimeout();
						activeAppContextSentForCurrentRecordingRef.current = null;
						send({ type: "RESPONSE_RECEIVED" });
					}

					// Fatal errors also trigger reconnection after showing the error
					if (errorData?.fatal) {
						send({
							type: "COMMUNICATION_ERROR",
							error: message,
						});
					}
				}
			},
			[send, displayState, clearResponseTimeout],
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
		match(displayState)
			.with("startingRecording", "recording", () => onStopRecording())
			.with("idle", () => onStartRecording())
			.with(
				"disconnected",
				"connecting",
				"reconnecting",
				"processing",
				() => {},
			)
			.exhaustive();
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

	// Determine view state for render
	const viewState = showError
		? ("error" as const)
		: isMicAcquiring
			? ("loading" as const)
			: match(displayState)
					.with(
						"startingRecording",
						"processing",
						"disconnected",
						"connecting",
						"reconnecting",
						() => "loading" as const,
					)
					.with("idle", "recording", () => "active" as const)
					.exhaustive();

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
			{match(viewState)
				.with("error", () => (
					<ErrorDisplay
						onDismiss={() => setShowError(false)}
						onStartRecording={
							displayState === "idle" ? onStartRecording : undefined
						}
					/>
				))
				.with("loading", () => LoadingSpinner)
				.with("active", () => (
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
				))
				.exhaustive()}
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
