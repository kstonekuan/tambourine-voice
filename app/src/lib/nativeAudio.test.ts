import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createNativeAudioBridge } from "./nativeAudio";

const tauriMockState = vi.hoisted(() => ({
	invokeMock: vi.fn(),
	listenMock: vi.fn(),
	unlistenMock: vi.fn(),
	nativeAudioDataListener: undefined as
		| undefined
		| ((event: { payload: number[] }) => void),
}));

vi.mock("@tauri-apps/api/core", () => ({
	invoke: (...args: unknown[]) => tauriMockState.invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
	listen: (...args: unknown[]) => tauriMockState.listenMock(...args),
}));

class MockAudioWorkletNode {
	public port = { postMessage: vi.fn() };

	public connect = vi.fn();
}

class MockAudioContext {
	public state: AudioContextState = "running";

	public audioWorklet = {
		addModule: vi.fn(async () => undefined),
	};

	public async resume(): Promise<void> {
		this.state = "running";
	}

	public createMediaStreamDestination(): MediaStreamAudioDestinationNode {
		return {
			stream: {
				getAudioTracks: () => [{} as MediaStreamTrack],
			},
		} as MediaStreamAudioDestinationNode;
	}
}

async function flushMicrotasks(turns = 3): Promise<void> {
	for (let index = 0; index < turns; index += 1) {
		await Promise.resolve();
	}
}

describe("createNativeAudioBridge", () => {
	beforeEach(() => {
		tauriMockState.invokeMock.mockReset();
		tauriMockState.listenMock.mockReset();
		tauriMockState.unlistenMock.mockReset();
		tauriMockState.nativeAudioDataListener = undefined;

		tauriMockState.invokeMock.mockResolvedValue(undefined);
		tauriMockState.listenMock.mockImplementation(
			async (
				_eventName: string,
				callback: (event: { payload: number[] }) => void,
			) => {
				tauriMockState.nativeAudioDataListener = callback;
				return tauriMockState.unlistenMock;
			},
		);

		globalThis.AudioContext =
			MockAudioContext as unknown as typeof AudioContext;
		globalThis.AudioWorkletNode =
			MockAudioWorkletNode as unknown as typeof AudioWorkletNode;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("returns first-frame-ready result when audio arrives before timeout", async () => {
		const nativeAudioBridge = await createNativeAudioBridge();
		const startPromise = nativeAudioBridge.start({
			deviceId: "test-device-id",
			waitForFirstAudioFrameMs: 500,
		});

		await flushMicrotasks();
		tauriMockState.nativeAudioDataListener?.({ payload: [0.1, 0.2] });
		const startResult = await startPromise;

		expect(startResult.firstFrameReceived).toBe(true);
		expect(startResult.timeToFirstFrameMs).not.toBeNull();
	});

	it("returns timeout result when no audio arrives after start", async () => {
		vi.useFakeTimers();
		const nativeAudioBridge = await createNativeAudioBridge();
		const startPromise = nativeAudioBridge.start({
			waitForFirstAudioFrameMs: 50,
		});

		await flushMicrotasks();
		await vi.advanceTimersByTimeAsync(50);
		const startResult = await startPromise;

		expect(startResult).toEqual({
			firstFrameReceived: false,
			timeToFirstFrameMs: null,
		});
	});

	it("resolves pending first-frame waiters and unlistens when stopped", async () => {
		const nativeAudioBridge = await createNativeAudioBridge();
		const pendingFirstFrameWaitPromise = nativeAudioBridge.start({
			waitForFirstAudioFrameMs: 500,
		});

		await flushMicrotasks();
		nativeAudioBridge.stop();
		const firstFrameWaitResult = await pendingFirstFrameWaitPromise;

		expect(firstFrameWaitResult).toEqual({
			firstFrameReceived: false,
			timeToFirstFrameMs: null,
		});
	});
});
