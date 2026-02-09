import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

const SAMPLE_RATE = 48000;
export const DEFAULT_FIRST_AUDIO_FRAME_TIMEOUT_MS = 500;

function getMonotonicTimestampMs(): number {
	return globalThis.performance?.now() ?? Date.now();
}

/**
 * Information about a native audio input device.
 */
export interface AudioDeviceInfo {
	/** Stable unique identifier (persists across reboots) */
	id: string;
	/** Human-readable device name for display */
	name: string;
}

/**
 * List available native audio input devices via cpal.
 * Returns device info with both stable ID and human-readable name.
 */
export async function listNativeAudioDevices(): Promise<AudioDeviceInfo[]> {
	return invoke<AudioDeviceInfo[]>("list_native_mic_devices");
}

export interface NativeAudioBridge {
	track: MediaStreamTrack;
	start: (options?: NativeMicStartOptions) => Promise<NativeMicStartResult>;
	stop: () => void;
	pause: () => void;
	resume: () => void;
}

export interface NativeMicStartOptions {
	/** Device ID to capture from; omitted means system default */
	deviceId?: string;
	/** Timeout for receiving the first native audio frame after start */
	waitForFirstAudioFrameMs?: number;
}

export interface NativeMicStartResult {
	/** True when at least one native audio frame arrived before timeout */
	firstFrameReceived: boolean;
	/** Time between wait-start and first frame arrival; null when timed out */
	timeToFirstFrameMs: number | null;
}

/**
 * Creates a MediaStreamTrack from native audio capture via cpal.
 *
 * Architecture:
 * Rust (cpal) → Tauri Event → AudioWorklet → MediaStreamDestination → MediaStreamTrack
 *
 * This bypasses the browser's getUserMedia() which has ~300-400ms latency on macOS
 * due to security overhead. Native capture via cpal has ~10-20ms latency.
 */
export async function createNativeAudioBridge(): Promise<NativeAudioBridge> {
	// Create AudioContext with matching sample rate
	const audioContext = new AudioContext({ sampleRate: SAMPLE_RATE });

	// Load AudioWorklet processor
	await audioContext.audioWorklet.addModule("/native-audio-processor.js");

	// Create worklet node
	const workletNode = new AudioWorkletNode(
		audioContext,
		"native-audio-processor",
	);

	// Create destination to get MediaStreamTrack
	const destination = audioContext.createMediaStreamDestination();
	workletNode.connect(destination);

	// Get the audio track
	const track = destination.stream.getAudioTracks()[0];
	if (!track) {
		throw new Error("Failed to create audio track from MediaStreamDestination");
	}

	// Listen for native audio data events
	let unlisten: UnlistenFn | undefined;
	type PendingFirstFrameWaiter = {
		startedAtMs: number;
		timeoutHandle: ReturnType<typeof setTimeout>;
		resolve: (result: NativeMicStartResult) => void;
	};
	const pendingFirstFrameWaiters = new Set<PendingFirstFrameWaiter>();

	const resolvePendingFirstFrameWaiters = (
		firstFrameReceived: boolean,
	): void => {
		const resolvedAtMs = getMonotonicTimestampMs();
		for (const pendingFirstFrameWaiter of pendingFirstFrameWaiters) {
			clearTimeout(pendingFirstFrameWaiter.timeoutHandle);
			pendingFirstFrameWaiters.delete(pendingFirstFrameWaiter);
			pendingFirstFrameWaiter.resolve({
				firstFrameReceived,
				timeToFirstFrameMs: firstFrameReceived
					? resolvedAtMs - pendingFirstFrameWaiter.startedAtMs
					: null,
			});
		}
	};

	const ensureNativeAudioListener = async (): Promise<void> => {
		if (unlisten) {
			return;
		}

		unlisten = await listen<number[]>("native-audio-data", (event) => {
			workletNode.port.postMessage({
				type: "audio-data",
				samples: event.payload,
			});
			resolvePendingFirstFrameWaiters(true);
		});
	};

	const waitForFirstAudioFrame = async (
		timeoutMs: number,
	): Promise<NativeMicStartResult> => {
		await ensureNativeAudioListener();

		return new Promise<NativeMicStartResult>((resolve) => {
			const pendingFirstFrameWaiter: PendingFirstFrameWaiter = {
				startedAtMs: getMonotonicTimestampMs(),
				timeoutHandle: setTimeout(() => {
					pendingFirstFrameWaiters.delete(pendingFirstFrameWaiter);
					resolve({
						firstFrameReceived: false,
						timeToFirstFrameMs: null,
					});
				}, timeoutMs),
				resolve,
			};
			pendingFirstFrameWaiters.add(pendingFirstFrameWaiter);
		});
	};

	const start = async (
		options?: NativeMicStartOptions,
	): Promise<NativeMicStartResult> => {
		// Resume audio context (required for autoplay policy)
		if (audioContext.state === "suspended") {
			await audioContext.resume();
		}

		await ensureNativeAudioListener();

		const deviceId = options?.deviceId;
		const firstAudioFrameTimeoutMs =
			options?.waitForFirstAudioFrameMs ?? DEFAULT_FIRST_AUDIO_FRAME_TIMEOUT_MS;

		// Start native capture
		await invoke("start_native_mic", { deviceId });

		return waitForFirstAudioFrame(firstAudioFrameTimeoutMs);
	};

	const stop = (): void => {
		invoke("stop_native_mic").catch((err) =>
			console.warn("[nativeAudio] stop_native_mic failed:", err),
		);
		resolvePendingFirstFrameWaiters(false);
		unlisten?.();
		unlisten = undefined;
	};

	const pause = (): void => {
		invoke("pause_native_mic").catch((err) =>
			console.warn("[nativeAudio] pause_native_mic failed:", err),
		);
	};

	const resume = (): void => {
		invoke("resume_native_mic").catch((err) =>
			console.warn("[nativeAudio] resume_native_mic failed:", err),
		);
	};

	return { track, start, stop, pause, resume };
}
