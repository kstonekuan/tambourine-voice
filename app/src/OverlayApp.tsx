import { Loader } from "@mantine/core";
import { useResizeObserver, useTimeout } from "@mantine/hooks";
import { PipecatClient, RTVIEvent } from "@pipecat-ai/client-js";
import {
	PipecatClientProvider,
	usePipecatClient,
} from "@pipecat-ai/client-react";
import { SmallWebRTCTransport } from "@pipecat-ai/small-webrtc-transport";
import { ThemeProvider, UserAudioComponent } from "@pipecat-ai/voice-ui-kit";
import { useDrag } from "@use-gesture/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { z } from "zod";
import Logo from "./assets/logo.svg?react";
import {
	useAddHistoryEntry,
	useServerUrl,
	useSetServerLLMProvider,
	useSetServerPromptSections,
	useSetServerSTTProvider,
	useSettings,
	useTypeText,
} from "./lib/queries";
import { type ConnectionState, tauriAPI } from "./lib/tauri";
import { useRecordingStore } from "./stores/recordingStore";
import "./app.css";

// Zod schemas for message validation
const TranscriptMessageSchema = z.object({
	type: z.literal("transcript"),
	text: z.string(),
});

const RecordingCompleteMessageSchema = z.object({
	type: z.literal("recording-complete"),
	hasContent: z.boolean().optional(),
});

function RecordingControl() {
	const client = usePipecatClient();
	const {
		state,
		setClient,
		startRecording,
		stopRecording,
		handleResponse,
		handleConnected,
		handleDisconnected,
	} = useRecordingStore();

	// Use Mantine's useResizeObserver hook
	const [containerRef, rect] = useResizeObserver();

	// Ref for tracking drag state
	const hasDragStartedRef = useRef(false);

	const { data: serverUrl } = useServerUrl();
	const { data: settings } = useSettings();

	// Track if we've ever connected (to distinguish initial connection from reconnection)
	const hasConnectedRef = useRef(false);

	// Simple connection: just call connect() when client and serverUrl are ready
	// SmallWebRTC handles reconnection internally (3 attempts)
	useEffect(() => {
		if (!client || !serverUrl) return;

		client
			.connect({ webrtcRequestParams: { endpoint: `${serverUrl}/api/offer` } })
			.catch((error: unknown) => {
				console.error("[Pipecat] Connection failed:", error);
			});
	}, [client, serverUrl]);

	// TanStack Query hooks
	const typeTextMutation = useTypeText();
	const addHistoryEntry = useAddHistoryEntry();
	const setServerPromptSections = useSetServerPromptSections();
	const setServerSTTProvider = useSetServerSTTProvider();
	const setServerLLMProvider = useSetServerLLMProvider();

	// Response timeout (10s)
	const { start: startResponseTimeout, clear: clearResponseTimeout } =
		useTimeout(() => {
			const currentState = useRecordingStore.getState().state;
			if (currentState === "processing") {
				handleResponse(); // Reset to idle
			}
		}, 10000);

	// Keep store client in sync
	useEffect(() => {
		setClient(client ?? null);
	}, [client, setClient]);

	// Emit connection state changes to other windows (main window)
	useEffect(() => {
		const unsubscribe = useRecordingStore.subscribe((newState, prevState) => {
			if (newState.state !== prevState.state) {
				tauriAPI.emitConnectionState(newState.state as ConnectionState);
			}
		});
		// Emit initial state (get from store directly to avoid dependency issues)
		const initialState = useRecordingStore.getState().state;
		tauriAPI.emitConnectionState(initialState as ConnectionState);
		return unsubscribe;
	}, []);

	// Auto-resize window to fit content using Mantine's useResizeObserver
	useEffect(() => {
		if (rect.width > 0 && rect.height > 0) {
			tauriAPI.resizeOverlay(Math.ceil(rect.width), Math.ceil(rect.height));
		}
	}, [rect.width, rect.height]);

	// Handle start/stop recording from hotkeys
	const onStartRecording = useCallback(async () => {
		await startRecording();
	}, [startRecording]);

	const onStopRecording = useCallback(() => {
		if (stopRecording()) {
			startResponseTimeout();
		}
	}, [stopRecording, startResponseTimeout]);

	// Hotkey event listeners
	useEffect(() => {
		let unlistenStart: (() => void) | undefined;
		let unlistenStop: (() => void) | undefined;

		const setup = async () => {
			unlistenStart = await tauriAPI.onStartRecording(onStartRecording);
			unlistenStop = await tauriAPI.onStopRecording(onStopRecording);
		};

		setup();

		return () => {
			unlistenStart?.();
			unlistenStop?.();
		};
	}, [onStartRecording, onStopRecording]);

	// Connection and response event handlers
	useEffect(() => {
		if (!client) return;

		const onConnected = () => {
			console.debug("[Pipecat] Connected");
			hasConnectedRef.current = true;
			handleConnected();

			// Sync cleanup prompt sections to server via REST API
			if (settings?.cleanup_prompt_sections) {
				setServerPromptSections.mutate(settings.cleanup_prompt_sections);
			}

			// Sync provider preferences to server
			if (settings?.stt_provider) {
				setServerSTTProvider.mutate(settings.stt_provider);
			}
			if (settings?.llm_provider) {
				setServerLLMProvider.mutate(settings.llm_provider);
			}
		};

		const onDisconnected = () => {
			console.debug("[Pipecat] Disconnected");

			// Check if we were recording/processing when disconnect happened
			const currentState = useRecordingStore.getState().state;
			if (currentState === "recording" || currentState === "processing") {
				console.warn("[Pipecat] Disconnected during recording/processing");
				try {
					client.enableMic(false);
					// Also stop the track to release the mic (removes OS mic indicator)
					const tracks = client.tracks();
					if (tracks?.local?.audio) {
						tracks.local.audio.stop();
					}
				} catch {
					// Ignore errors when cleaning up mic
				}
			}

			handleDisconnected();

			// SmallWebRTC already tried to reconnect (3 attempts) and gave up
			// Keep retrying with the same client - must call disconnect() first to reset state
			if (hasConnectedRef.current && serverUrl) {
				setTimeout(async () => {
					try {
						await client.disconnect();
						await client.connect({
							webrtcRequestParams: { endpoint: `${serverUrl}/api/offer` },
						});
					} catch (error: unknown) {
						console.error("[Pipecat] Reconnection failed:", error);
					}
				}, 3000);
			}
		};

		const onServerMessage = async (message: unknown) => {
			const transcriptResult = TranscriptMessageSchema.safeParse(message);
			if (transcriptResult.success) {
				clearResponseTimeout();
				const { text } = transcriptResult.data;
				console.debug("[Pipecat] Transcript:", text);
				try {
					await typeTextMutation.mutateAsync(text);
				} catch (error) {
					console.error("[Pipecat] Failed to type text:", error);
				}
				addHistoryEntry.mutate(text);
				handleResponse();
				return;
			}

			const recordingCompleteResult =
				RecordingCompleteMessageSchema.safeParse(message);
			if (recordingCompleteResult.success) {
				clearResponseTimeout();
				handleResponse();
			}
		};

		const onError = (error: unknown) => {
			console.error("[Pipecat] Error:", error);
		};

		const onDeviceError = (error: unknown) => {
			console.error("[Pipecat] Device error:", error);
		};

		client.on(RTVIEvent.Connected, onConnected);
		client.on(RTVIEvent.Disconnected, onDisconnected);
		client.on(RTVIEvent.ServerMessage, onServerMessage);
		client.on(RTVIEvent.Error, onError);
		client.on(RTVIEvent.DeviceError, onDeviceError);

		return () => {
			client.off(RTVIEvent.Connected, onConnected);
			client.off(RTVIEvent.Disconnected, onDisconnected);
			client.off(RTVIEvent.ServerMessage, onServerMessage);
			client.off(RTVIEvent.Error, onError);
			client.off(RTVIEvent.DeviceError, onDeviceError);
		};
	}, [
		client,
		serverUrl,
		settings,
		handleConnected,
		handleDisconnected,
		handleResponse,
		typeTextMutation,
		addHistoryEntry,
		clearResponseTimeout,
		setServerPromptSections,
		setServerSTTProvider,
		setServerLLMProvider,
	]);

	// Click handler (toggle mode)
	const handleClick = useCallback(() => {
		if (state === "recording") {
			onStopRecording();
		} else if (state === "idle") {
			onStartRecording();
		}
	}, [state, onStartRecording, onStopRecording]);

	// Drag handler using @use-gesture/react
	// Handles unfocused window dragging (data-tauri-drag-region doesn't work on unfocused windows)
	const bindDrag = useDrag(
		({ movement: [mx, my], first, last, memo }) => {
			if (first) {
				hasDragStartedRef.current = false;
				return false; // memo = false (hasn't started dragging)
			}

			const distance = Math.sqrt(mx * mx + my * my);
			const DRAG_THRESHOLD = 5;

			// Start dragging once threshold is exceeded
			if (!memo && distance > DRAG_THRESHOLD) {
				hasDragStartedRef.current = true;
				tauriAPI.startDragging();
				return true; // memo = true (dragging started)
			}

			if (last) {
				hasDragStartedRef.current = false;
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
				padding: 2,
				cursor: "grab",
				userSelect: "none",
			}}
		>
			{state === "processing" ||
				state === "disconnected" ||
				state === "connecting" ? (
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
			) : (
				<UserAudioComponent
					onClick={handleClick}
					isMicEnabled={state === "recording"}
					noIcon={true}
					noDevicePicker={true}
					noVisualizer={state !== "recording"}
					visualizerProps={{
						barColor: "#eeeeee",
						backgroundColor: "#000000",
					}}
					classNames={{
						button: "bg-black text-white hover:bg-gray-900",
					}}
				>
					{state !== "recording" && <Logo className="size-5" />}
				</UserAudioComponent>
			)}
		</div>
	);
}

export default function OverlayApp() {
	const [client, setClient] = useState<PipecatClient | null>(null);
	const [devicesReady, setDevicesReady] = useState(false);
	const { data: settings } = useSettings();

	// Initial client creation on mount
	useEffect(() => {
		const transport = new SmallWebRTCTransport({
			iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
		});
		const pipecatClient = new PipecatClient({
			transport,
			enableMic: false,
			enableCam: false,
		});
		setClient(pipecatClient);

		pipecatClient
			.initDevices()
			.then(() => {
				setDevicesReady(true);
			})
			.catch((error: unknown) => {
				console.error("[Pipecat] Failed to initialize devices:", error);
				setDevicesReady(true);
			});

		return () => {
			pipecatClient.disconnect().catch(() => { });
		};
	}, []);

	// Apply selected microphone when settings or client changes
	useEffect(() => {
		if (client && devicesReady && settings?.selected_mic_id) {
			client.updateMic(settings.selected_mic_id);
		}
	}, [client, devicesReady, settings?.selected_mic_id]);

	if (!client || !devicesReady) {
		return (
			<div
				className="flex items-center justify-center"
				style={{
					width: 48,
					height: 48,
					backgroundColor: "rgba(0, 0, 0, 0.9)",
					borderRadius: 12,
				}}
			>
				<Loader size="sm" color="white" />
			</div>
		);
	}

	return (
		<ThemeProvider>
			<PipecatClientProvider client={client}>
				<RecordingControl />
			</PipecatClientProvider>
		</ThemeProvider>
	);
}
