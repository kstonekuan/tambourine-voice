import { useCallback, useEffect, useRef, useState } from "react";
import {
	createNativeAudioBridge,
	type NativeAudioBridge,
	type NativeMicStartOptions,
	type NativeMicStartResult,
} from "../lib/nativeAudio";

interface UseNativeAudioTrackResult {
	/** The MediaStreamTrack from native audio capture, or null if not ready */
	track: MediaStreamTrack | null;
	/** Resolve the latest track value from the native bridge */
	getCurrentTrack: () => MediaStreamTrack | null;
	/** Whether the native audio bridge has been initialized */
	isReady: boolean;
	/** Error if initialization failed */
	error: Error | null;
	/** Wait until the native bridge is initialized */
	waitUntilReady: (timeoutMs: number) => Promise<void>;
	/** Start capturing audio from the specified device (by ID) */
	start: (options?: NativeMicStartOptions) => Promise<NativeMicStartResult>;
	/** Stop capturing and release resources */
	stop: () => void;
	/** Pause capture (stream stays alive for fast resume) */
	pause: () => void;
	/** Resume capture after pause */
	resume: () => void;
}

/**
 * React hook for native audio capture via cpal.
 *
 * Initializes the native audio bridge on mount and provides
 * a MediaStreamTrack that can be used with WebRTC.
 *
 * This bypasses the browser's getUserMedia() which has ~300-400ms latency
 * on macOS due to security overhead. Native capture has ~10-20ms latency.
 */
export function useNativeAudioTrack(): UseNativeAudioTrackResult {
	const bridgeRef = useRef<NativeAudioBridge | null>(null);
	const [isReady, setIsReady] = useState(false);
	const [error, setError] = useState<Error | null>(null);
	const initializationErrorRef = useRef<Error | null>(null);

	useEffect(() => {
		let mounted = true;

		const init = async () => {
			try {
				const bridge = await createNativeAudioBridge();
				if (mounted) {
					bridgeRef.current = bridge;
					setIsReady(true);
				}
			} catch (err) {
				console.error("[NativeAudio] Failed to initialize:", err);
				if (mounted) {
					const nativeAudioInitializationError =
						err instanceof Error ? err : new Error(String(err));
					initializationErrorRef.current = nativeAudioInitializationError;
					setError(nativeAudioInitializationError);
				}
			}
		};

		init();

		return () => {
			mounted = false;
			bridgeRef.current?.stop();
		};
	}, []);

	const waitUntilReady = useCallback(async (timeoutMs: number) => {
		const waitStartAtMs = Date.now();
		const waitIntervalMs = 10;

		while (!bridgeRef.current) {
			if (initializationErrorRef.current) {
				throw initializationErrorRef.current;
			}

			if (Date.now() - waitStartAtMs >= timeoutMs) {
				throw new Error(`Native audio bridge not ready within ${timeoutMs}ms`);
			}

			await new Promise((resolve) => setTimeout(resolve, waitIntervalMs));
		}
	}, []);

	const start = useCallback(async (options?: NativeMicStartOptions) => {
		const nativeAudioBridge = bridgeRef.current;
		if (!nativeAudioBridge) {
			throw new Error("Native audio bridge is not ready");
		}

		return nativeAudioBridge.start(options);
	}, []);

	const getCurrentTrack = useCallback(() => {
		return bridgeRef.current?.track ?? null;
	}, []);

	const stop = useCallback(() => {
		bridgeRef.current?.stop();
	}, []);

	const pause = useCallback(() => {
		bridgeRef.current?.pause();
	}, []);

	const resume = useCallback(() => {
		bridgeRef.current?.resume();
	}, []);

	return {
		track: bridgeRef.current?.track ?? null,
		getCurrentTrack,
		isReady,
		error,
		waitUntilReady,
		start,
		stop,
		pause,
		resume,
	};
}
