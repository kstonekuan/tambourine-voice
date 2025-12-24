import { Loader } from "@mantine/core";
import { useResizeObserver, useTimeout } from "@mantine/hooks";
import {
	type BotLLMTextData,
	PipecatClient,
	RTVIEvent,
} from "@pipecat-ai/client-js";
import {
	PipecatClientProvider,
	usePipecatClient,
	useRTVIClientEvent,
} from "@pipecat-ai/client-react";
import { SmallWebRTCTransport } from "@pipecat-ai/small-webrtc-transport";
import { ThemeProvider, UserAudioComponent } from "@pipecat-ai/voice-ui-kit";
import { useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useDrag } from "@use-gesture/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { z } from "zod";
import Logo from "./assets/logo.svg?react";
import {
	useAddHistoryEntry,
	useServerUrl,
	useSettings,
	useTypeText,
} from "./lib/queries";
import {
	type CleanupPromptSections,
	type ConnectionState,
	tauriAPI,
} from "./lib/tauri";
import { useRecordingStore } from "./stores/recordingStore";
import "./overlay-global.css";

// Zod schemas for message validation
const RecordingCompleteMessageSchema = z.object({
	type: z.literal("recording-complete"),
	hasContent: z.boolean().optional(),
});

// Config response schemas (relayed to main window for notifications)
const ConfigUpdatedMessageSchema = z.object({
	type: z.literal("config-updated"),
	setting: z.string(),
	value: z.unknown(),
	success: z.literal(true),
});

const ConfigErrorMessageSchema = z.object({
	type: z.literal("config-error"),
	setting: z.string(),
	error: z.string(),
});

// Available providers schema (relayed to main window for settings UI)
const AvailableProvidersMessageSchema = z.object({
	type: z.literal("available-providers"),
	stt: z.array(
		z.object({
			value: z.string(),
			label: z.string(),
			is_local: z.boolean(),
			model: z.string().nullable(),
		}),
	),
	llm: z.array(
		z.object({
			value: z.string(),
			label: z.string(),
			is_local: z.boolean(),
			model: z.string().nullable(),
		}),
	),
});

// Non-empty array type for type-safe batched sends
type NonEmptyArray<T> = [T, ...T[]];

// Discriminated union for type-safe config messages
type ConfigMessage =
	| { type: "set-prompt-sections"; data: { sections: CleanupPromptSections } }
	| { type: "set-stt-provider"; data: { provider: string } }
	| { type: "set-llm-provider"; data: { provider: string } }
	| { type: "set-stt-timeout"; data: { timeout_seconds: number } };

// Helper to send multiple config messages - only callable with non-empty list
function sendConfigMessages(
	client: PipecatClient,
	messages: NonEmptyArray<ConfigMessage>,
) {
	for (const { type, data } of messages) {
		client.sendClientMessage(type, data);
	}
}

function RecordingControl() {
	const client = usePipecatClient();
	const queryClient = useQueryClient();
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

	// Accumulate LLM text chunks (RTVIObserver streams text in chunks)
	const llmTextAccumulatorRef = useRef("");

	// Track previous settings to detect actual changes (for syncing while connected)
	const prevSettingsRef = useRef(settings);

	// Initial connection: triggered when client and serverUrl are ready
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
	const buildConfigMessages = useCallback(
		(
			currentSettings: typeof settings,
			prevSettings?: typeof settings,
		): ConfigMessage[] => {
			const messages: ConfigMessage[] = [];

			const hasChanged = (
				key: keyof NonNullable<typeof settings>,
				useJsonCompare = false,
			) => {
				const current = currentSettings?.[key];
				const prev = prevSettings?.[key];
				if (current == null) return false;
				if (prevSettings === undefined) return true; // Initial sync
				return useJsonCompare
					? JSON.stringify(current) !== JSON.stringify(prev)
					: current !== prev;
			};

			if (hasChanged("cleanup_prompt_sections", true)) {
				messages.push({
					type: "set-prompt-sections",
					data: {
						sections:
							currentSettings?.cleanup_prompt_sections as CleanupPromptSections,
					},
				});
			}
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
			if (hasChanged("stt_timeout_seconds")) {
				messages.push({
					type: "set-stt-timeout",
					data: {
						timeout_seconds: currentSettings?.stt_timeout_seconds as number,
					},
				});
			}

			return messages;
		},
		[],
	);

	// Connection event handler
	useRTVIClientEvent(
		RTVIEvent.Connected,
		useCallback(() => {
			console.debug("[Pipecat] Connected");
			hasConnectedRef.current = true;
			handleConnected();

			// Sync settings to server via data channel (with delay to ensure connection is stable)
			setTimeout(() => {
				if (!client) return;

				// Request available providers (for settings UI in main window)
				client.sendClientMessage("get-available-providers", {});

				// Send all current settings
				const messages = buildConfigMessages(settings);
				if (messages.length > 0) {
					sendConfigMessages(client, messages as NonEmptyArray<ConfigMessage>);
				}
			}, 1000);
		}, [client, settings, handleConnected, buildConfigMessages]),
	);

	// Sync settings when they change WHILE already connected
	useEffect(() => {
		const prevSettings = prevSettingsRef.current;
		prevSettingsRef.current = settings;

		// Only sync if connected AND settings actually changed
		if (!client || state !== "idle") return;
		if (prevSettings === settings) return;

		const messages = buildConfigMessages(settings, prevSettings);
		if (messages.length > 0) {
			sendConfigMessages(client, messages as NonEmptyArray<ConfigMessage>);
		}
	}, [client, state, settings, buildConfigMessages]);

	// Disconnection event handler
	// Handles cleanup, state transition, and reconnection
	useRTVIClientEvent(
		RTVIEvent.Disconnected,
		useCallback(() => {
			console.debug("[Pipecat] Disconnected");

			// Check if we were recording/processing when disconnect happened
			const currentState = useRecordingStore.getState().state;
			if (currentState === "recording" || currentState === "processing") {
				console.warn("[Pipecat] Disconnected during recording/processing");
				try {
					client?.enableMic(false);
					// Also stop the track to release the mic (removes OS mic indicator)
					const tracks = client?.tracks();
					if (tracks?.local?.audio) {
						tracks.local.audio.stop();
					}
				} catch {
					// Ignore errors when cleaning up mic
				}
			}

			handleDisconnected();

			// Reconnection: only if we've connected before (not on initial connection failure)
			// SmallWebRTC already tried to reconnect (3 attempts) and gave up
			if (hasConnectedRef.current && serverUrl && client) {
				setTimeout(async () => {
					try {
						await client.disconnect(); // Reset client state
						await client.connect({
							webrtcRequestParams: { endpoint: `${serverUrl}/api/offer` },
						});
					} catch (error: unknown) {
						console.error("[Pipecat] Reconnection failed:", error);
					}
				}, 3000);
			}
		}, [client, serverUrl, handleDisconnected]),
	);

	// LLM text streaming handlers (using official RTVI protocol via RTVIObserver)
	useRTVIClientEvent(
		RTVIEvent.BotLlmStarted,
		useCallback(() => {
			// Reset accumulator when LLM starts generating
			llmTextAccumulatorRef.current = "";
		}, []),
	);

	useRTVIClientEvent(
		RTVIEvent.BotLlmText,
		useCallback((data: BotLLMTextData) => {
			// Accumulate text chunks from LLM
			llmTextAccumulatorRef.current += data.text;
		}, []),
	);

	useRTVIClientEvent(
		RTVIEvent.BotLlmStopped,
		useCallback(async () => {
			clearResponseTimeout();
			const text = llmTextAccumulatorRef.current.trim();
			llmTextAccumulatorRef.current = "";

			if (text) {
				console.debug("[Pipecat] LLM response:", text);
				try {
					await typeTextMutation.mutateAsync(text);
				} catch (error) {
					console.error("[Pipecat] Failed to type text:", error);
				}
				addHistoryEntry.mutate(text);
			}
			handleResponse();
		}, [
			clearResponseTimeout,
			typeTextMutation,
			addHistoryEntry,
			handleResponse,
		]),
	);

	// Server message handler (for custom messages: config-updated, recording-complete, etc.)
	useRTVIClientEvent(
		RTVIEvent.ServerMessage,
		useCallback(
			(message: unknown) => {
				const recordingCompleteResult =
					RecordingCompleteMessageSchema.safeParse(message);
				if (recordingCompleteResult.success) {
					clearResponseTimeout();
					handleResponse();
					return;
				}

				// Config response messages - relay to main window for notifications
				const configUpdatedResult =
					ConfigUpdatedMessageSchema.safeParse(message);
				if (configUpdatedResult.success) {
					tauriAPI.emitConfigResponse({
						type: "config-updated",
						setting: configUpdatedResult.data.setting,
						value: configUpdatedResult.data.value,
					});
					return;
				}

				const configErrorResult = ConfigErrorMessageSchema.safeParse(message);
				if (configErrorResult.success) {
					tauriAPI.emitConfigResponse({
						type: "config-error",
						setting: configErrorResult.data.setting,
						error: configErrorResult.data.error,
					});
					return;
				}

				// Available providers - relay to main window for settings UI
				const availableProvidersResult =
					AvailableProvidersMessageSchema.safeParse(message);
				if (availableProvidersResult.success) {
					tauriAPI.emitAvailableProviders({
						stt: availableProvidersResult.data.stt,
						llm: availableProvidersResult.data.llm,
					});
					return;
				}
			},
			[clearResponseTimeout, handleResponse],
		),
	);

	// Error handlers
	useRTVIClientEvent(
		RTVIEvent.Error,
		useCallback((error: unknown) => {
			console.error("[Pipecat] Error:", error);
		}, []),
	);

	useRTVIClientEvent(
		RTVIEvent.DeviceError,
		useCallback((error: unknown) => {
			console.error("[Pipecat] Device error:", error);
		}, []),
	);

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
				border: "1px solid rgba(128, 128, 128, 0.9)",
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
				setDevicesReady(false);
			});

		return () => {
			pipecatClient.disconnect().catch(() => {});
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
				<Loader size="xs" color="white" />
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
