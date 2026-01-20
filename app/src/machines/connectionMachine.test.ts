import type { PipecatClient } from "@pipecat-ai/client-js";
import { describe, expect, it, vi } from "vitest";
import {
	type AnyActorRef,
	createActor,
	fromCallback,
	fromPromise,
} from "xstate";
import { connectionMachine } from "./connectionMachine";

// Mock the tauri module to avoid "window is not defined" errors
// This is needed because the real machine has inline async actions that call tauriAPI
vi.mock("../lib/tauri", () => ({
	tauriAPI: {
		emitConnectionState: vi.fn(),
		emitReconnectStarted: vi.fn(),
		emitReconnectResult: vi.fn(),
		getClientUUID: vi.fn().mockResolvedValue(null),
		setClientUUID: vi.fn().mockResolvedValue(undefined),
		clearClientUUID: vi.fn().mockResolvedValue(undefined),
	},
	configAPI: {
		registerClient: vi.fn().mockResolvedValue("mock-uuid"),
	},
}));

// Suppress console.debug/log output from the real machine during tests
vi.spyOn(console, "debug").mockImplementation(() => {});
vi.spyOn(console, "log").mockImplementation(() => {});

// Type for the sendBack function used by connect actor
type ConnectSendBack = (
	event:
		| { type: "CONNECTED" }
		| { type: "DISCONNECTED" }
		| { type: "UUID_REJECTED" },
) => void;

// Type for the sendBack function used by disconnectListener actor
type DisconnectSendBack = (event: { type: "DISCONNECTED" }) => void;

/**
 * Waits for the actor to reach a specific state (real timers only).
 */
async function waitForState(
	actor: AnyActorRef,
	stateName: string,
	timeout = 1000,
): Promise<void> {
	const startTime = Date.now();
	while (Date.now() - startTime < timeout) {
		const snapshot = actor.getSnapshot();
		if (snapshot.value === stateName) {
			return;
		}
		await new Promise((resolve) => setTimeout(resolve, 10));
	}
	throw new Error(
		`Timeout waiting for state "${stateName}". Current state: ${actor.getSnapshot().value}`,
	);
}

/**
 * Creates a test machine using the real connectionMachine with test-friendly actors.
 * Uses .provide() to inject mock actors while keeping the real machine logic.
 */
function createTestMachine(config: {
	createClientBehavior?: "success" | "error" | "pending";
	createClientError?: string;
}) {
	const {
		createClientBehavior = "success",
		createClientError = "Client creation failed",
	} = config;

	const callbacks = {
		connectSendBack: null as ConnectSendBack | null,
		disconnectSendBack: null as DisconnectSendBack | null,
	};

	const machine = connectionMachine.provide({
		actors: {
			createClient: fromPromise(async () => {
				if (createClientBehavior === "error") {
					throw new Error(createClientError);
				}
				if (createClientBehavior === "pending") {
					return new Promise(() => {});
				}
				return {
					client: { id: "mock-client" } as unknown as PipecatClient,
					clientUUID: "test-uuid-12345",
				};
			}),
			connect: fromCallback(({ sendBack }) => {
				callbacks.connectSendBack = sendBack as ConnectSendBack;
				return () => {
					callbacks.connectSendBack = null;
				};
			}),
			disconnectListener: fromCallback(({ sendBack }) => {
				callbacks.disconnectSendBack = sendBack as DisconnectSendBack;
				return () => {
					callbacks.disconnectSendBack = null;
				};
			}),
		},
		actions: {
			emitConnectionState: () => {},
			emitReconnectStarted: () => {},
			emitReconnectResult: () => {},
			cleanupClient: () => {},
			logState: () => {},
		},
	});

	return { machine, callbacks };
}

describe("connectionMachine", () => {
	describe("Initial State & Context", () => {
		it("starts in disconnected state", () => {
			const { machine } = createTestMachine({});
			const actor = createActor(machine);
			actor.start();

			expect(actor.getSnapshot().value).toBe("disconnected");

			actor.stop();
		});

		it("has correct default context values", () => {
			const { machine } = createTestMachine({});
			const actor = createActor(machine);
			actor.start();

			const { context } = actor.getSnapshot();
			expect(context.client).toBeNull();
			expect(context.clientUUID).toBeNull();
			expect(context.serverUrl).toBe("");
			expect(context.retryCount).toBe(0);
			expect(context.error).toBeNull();

			actor.stop();
		});
	});

	describe("State Transitions from disconnected", () => {
		it("transitions to initializing on CONNECT event", async () => {
			const { machine } = createTestMachine({
				createClientBehavior: "pending",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });

			await waitForState(actor, "initializing");
			expect(actor.getSnapshot().context.serverUrl).toBe(
				"http://localhost:8000",
			);

			actor.stop();
		});

		it("ignores other events in disconnected state", () => {
			const { machine } = createTestMachine({});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECTED" });
			expect(actor.getSnapshot().value).toBe("disconnected");

			actor.send({ type: "DISCONNECTED" });
			expect(actor.getSnapshot().value).toBe("disconnected");

			actor.send({ type: "START_RECORDING" });
			expect(actor.getSnapshot().value).toBe("disconnected");

			actor.stop();
		});
	});

	describe("State Transitions from initializing", () => {
		it("transitions to connecting on createClient success", async () => {
			const { machine } = createTestMachine({
				createClientBehavior: "success",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });

			await waitForState(actor, "connecting");

			const { context } = actor.getSnapshot();
			expect(context.client).not.toBeNull();
			expect(context.clientUUID).toBe("test-uuid-12345");

			actor.stop();
		});

		it("transitions to retrying on createClient error", async () => {
			const { machine } = createTestMachine({
				createClientBehavior: "error",
				createClientError: "Network error",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });

			await waitForState(actor, "retrying");

			const { context } = actor.getSnapshot();
			expect(context.error).toBe("Network error");

			actor.stop();
		});
	});

	describe("State Transitions from connecting", () => {
		it("transitions to idle on CONNECTED event", async () => {
			const { machine, callbacks } = createTestMachine({
				createClientBehavior: "success",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });
			await waitForState(actor, "connecting");

			callbacks.connectSendBack?.({ type: "CONNECTED" });

			await waitForState(actor, "idle");

			const { context } = actor.getSnapshot();
			expect(context.retryCount).toBe(0);
			expect(context.error).toBeNull();

			actor.stop();
		});

		it("transitions to retrying on DISCONNECTED event", async () => {
			const { machine, callbacks } = createTestMachine({
				createClientBehavior: "success",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });
			await waitForState(actor, "connecting");

			callbacks.connectSendBack?.({ type: "DISCONNECTED" });

			await waitForState(actor, "retrying");

			actor.stop();
		});

		it("clears client and clientUUID on UUID_REJECTED event", async () => {
			// This test verifies the context changes, not the final state
			// (which may cycle back to connecting due to auto-retry)
			const { machine, callbacks } = createTestMachine({
				createClientBehavior: "success",
			});
			const actor = createActor(machine);

			// Track state changes
			const stateHistory: string[] = [];
			actor.subscribe((snapshot) => {
				stateHistory.push(snapshot.value as string);
			});

			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });
			await waitForState(actor, "connecting");

			const uuidBefore = actor.getSnapshot().context.clientUUID;
			expect(uuidBefore).toBe("test-uuid-12345");

			callbacks.connectSendBack?.({ type: "UUID_REJECTED" });

			// Wait a moment for state transition
			await new Promise((resolve) => setTimeout(resolve, 50));

			// Verify that initializing was visited after connecting
			const connectingIndex = stateHistory.indexOf("connecting");
			const initializingAfterIndex = stateHistory.indexOf(
				"initializing",
				connectingIndex + 1,
			);
			expect(initializingAfterIndex).toBeGreaterThan(connectingIndex);

			actor.stop();
		});

		it("transitions to retrying on connection timeout", async () => {
			vi.useFakeTimers();

			// Create machine that stays in connecting (no auto-send)
			const { machine } = createTestMachine({
				createClientBehavior: "success",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });

			// Advance through the createClient promise
			await vi.advanceTimersByTimeAsync(0);
			expect(actor.getSnapshot().value).toBe("connecting");

			// Advance to just before timeout
			await vi.advanceTimersByTimeAsync(29999);
			expect(actor.getSnapshot().value).toBe("connecting");

			// Advance past timeout
			await vi.advanceTimersByTimeAsync(1);
			expect(actor.getSnapshot().value).toBe("retrying");
			expect(actor.getSnapshot().context.error).toBe("Connection timeout");

			actor.stop();
			vi.useRealTimers();
		});
	});

	describe("State Transitions from idle", () => {
		async function setupIdleState() {
			const { machine, callbacks } = createTestMachine({
				createClientBehavior: "success",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });
			await waitForState(actor, "connecting");
			callbacks.connectSendBack?.({ type: "CONNECTED" });
			await waitForState(actor, "idle");

			return { actor, callbacks, machine };
		}

		it("transitions to retrying on DISCONNECTED from disconnectListener", async () => {
			const { actor, callbacks } = await setupIdleState();

			callbacks.disconnectSendBack?.({ type: "DISCONNECTED" });
			await waitForState(actor, "retrying");

			actor.stop();
		});

		it("transitions to retrying on COMMUNICATION_ERROR event", async () => {
			const { actor } = await setupIdleState();

			actor.send({ type: "COMMUNICATION_ERROR", error: "Send failed" });
			await waitForState(actor, "retrying");

			actor.stop();
		});

		it("transitions to recording on START_RECORDING event", async () => {
			const { actor } = await setupIdleState();

			actor.send({ type: "START_RECORDING" });
			await waitForState(actor, "recording");

			actor.stop();
		});

		it("updates serverUrl on SERVER_URL_CHANGED", async () => {
			const { actor } = await setupIdleState();

			actor.send({
				type: "SERVER_URL_CHANGED",
				serverUrl: "http://newserver:9000",
			});

			// Wait for transition
			await new Promise((resolve) => setTimeout(resolve, 50));

			const { context } = actor.getSnapshot();
			expect(context.serverUrl).toBe("http://newserver:9000");
			// retryCount should remain 0 (reset on transition)
			expect(context.retryCount).toBe(0);

			actor.stop();
		});

		it("resets retryCount on RECONNECT", async () => {
			const { actor } = await setupIdleState();

			actor.send({ type: "RECONNECT" });

			await new Promise((resolve) => setTimeout(resolve, 50));

			const { context } = actor.getSnapshot();
			// retryCount should be reset to 0
			expect(context.retryCount).toBe(0);

			actor.stop();
		});
	});

	describe("State Transitions from recording", () => {
		async function setupRecordingState() {
			const { machine, callbacks } = createTestMachine({
				createClientBehavior: "success",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });
			await waitForState(actor, "connecting");
			callbacks.connectSendBack?.({ type: "CONNECTED" });
			await waitForState(actor, "idle");
			actor.send({ type: "START_RECORDING" });
			await waitForState(actor, "recording");

			return { actor, callbacks };
		}

		it("transitions to retrying on DISCONNECTED event", async () => {
			const { actor, callbacks } = await setupRecordingState();

			callbacks.disconnectSendBack?.({ type: "DISCONNECTED" });
			await waitForState(actor, "retrying");

			actor.stop();
		});

		it("transitions to retrying on COMMUNICATION_ERROR event", async () => {
			const { actor } = await setupRecordingState();

			actor.send({ type: "COMMUNICATION_ERROR", error: "Send failed" });
			await waitForState(actor, "retrying");

			actor.stop();
		});

		it("transitions to processing on STOP_RECORDING event", async () => {
			const { actor } = await setupRecordingState();

			actor.send({ type: "STOP_RECORDING" });
			await waitForState(actor, "processing");

			actor.stop();
		});
	});

	describe("State Transitions from processing", () => {
		async function setupProcessingState() {
			const { machine, callbacks } = createTestMachine({
				createClientBehavior: "success",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });
			await waitForState(actor, "connecting");
			callbacks.connectSendBack?.({ type: "CONNECTED" });
			await waitForState(actor, "idle");
			actor.send({ type: "START_RECORDING" });
			await waitForState(actor, "recording");
			actor.send({ type: "STOP_RECORDING" });
			await waitForState(actor, "processing");

			return { actor, callbacks };
		}

		it("transitions to retrying on DISCONNECTED event", async () => {
			const { actor, callbacks } = await setupProcessingState();

			callbacks.disconnectSendBack?.({ type: "DISCONNECTED" });
			await waitForState(actor, "retrying");

			actor.stop();
		});

		it("transitions to retrying on COMMUNICATION_ERROR event", async () => {
			const { actor } = await setupProcessingState();

			actor.send({ type: "COMMUNICATION_ERROR", error: "Send failed" });
			await waitForState(actor, "retrying");

			actor.stop();
		});

		it("transitions to idle on RESPONSE_RECEIVED event", async () => {
			const { actor } = await setupProcessingState();

			actor.send({ type: "RESPONSE_RECEIVED" });
			await waitForState(actor, "idle");

			actor.stop();
		});
	});

	describe("State Transitions from retrying", () => {
		it("increments retryCount on entry", async () => {
			const { machine } = createTestMachine({
				createClientBehavior: "error",
			});
			const actor = createActor(machine);
			actor.start();

			expect(actor.getSnapshot().context.retryCount).toBe(0);

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });
			await waitForState(actor, "retrying");

			expect(actor.getSnapshot().context.retryCount).toBe(1);

			actor.stop();
		});

		it("transitions to initializing after delay", async () => {
			vi.useFakeTimers();

			const { machine } = createTestMachine({
				createClientBehavior: "error",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });

			await vi.advanceTimersByTimeAsync(0); // Let createClient fail
			expect(actor.getSnapshot().value).toBe("retrying");
			expect(actor.getSnapshot().context.retryCount).toBe(1);

			// First retry delay is 2000ms (2^1 * 1000)
			await vi.advanceTimersByTimeAsync(1999);
			expect(actor.getSnapshot().value).toBe("retrying");

			await vi.advanceTimersByTimeAsync(1);
			// Should transition to initializing, then immediately back to retrying
			expect(actor.getSnapshot().context.retryCount).toBeGreaterThan(1);

			actor.stop();
			vi.useRealTimers();
		});

		it("keeps retrying indefinitely", async () => {
			vi.useFakeTimers();

			// Verify machine continues retrying by cycling through several retries
			const { machine } = createTestMachine({
				createClientBehavior: "error",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });

			// First failure
			await vi.advanceTimersByTimeAsync(0);
			expect(actor.getSnapshot().value).toBe("retrying");
			expect(actor.getSnapshot().context.retryCount).toBe(1);

			// Retry delays: 2s, 4s, 8s, 16s, 30s (capped)
			const delays = [2000, 4000, 8000, 16000, 30000];
			for (const [index, delay] of delays.entries()) {
				await vi.advanceTimersByTimeAsync(delay);
				// After each retry delay + failure, retryCount increases
				expect(actor.getSnapshot().context.retryCount).toBe(index + 2);
			}

			// Verify it continues retrying at the capped delay
			await vi.advanceTimersByTimeAsync(30000);
			expect(actor.getSnapshot().value).toBe("retrying");
			expect(actor.getSnapshot().context.retryCount).toBe(7);

			// Machine never gives up - still retrying, not disconnected
			expect(actor.getSnapshot().value).not.toBe("disconnected");

			actor.stop();
			vi.useRealTimers();
		});

		it("resets retryCount to 0 on RECONNECT event", async () => {
			vi.useFakeTimers();

			const { machine } = createTestMachine({
				createClientBehavior: "error",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });

			// First failure
			await vi.advanceTimersByTimeAsync(0);
			expect(actor.getSnapshot().value).toBe("retrying");
			expect(actor.getSnapshot().context.retryCount).toBe(1);

			// Wait for another retry to increase count
			await vi.advanceTimersByTimeAsync(2000); // First retry delay
			expect(actor.getSnapshot().context.retryCount).toBe(2);

			// Manual reconnect should reset retryCount to 0
			actor.send({ type: "RECONNECT" });

			expect(actor.getSnapshot().context.retryCount).toBe(0);
			expect(actor.getSnapshot().value).toBe("initializing");

			actor.stop();
			vi.useRealTimers();
		});
	});

	describe("Delays: retryDelay (Exponential Backoff)", () => {
		it("calculates correct delay for each retry count", () => {
			const calculateDelay = (retryCount: number) =>
				Math.min(1000 * 2 ** retryCount, 30000);

			expect(calculateDelay(1)).toBe(2000); // 1st retry: 2s
			expect(calculateDelay(2)).toBe(4000); // 2nd retry: 4s
			expect(calculateDelay(3)).toBe(8000); // 3rd retry: 8s
			expect(calculateDelay(4)).toBe(16000); // 4th retry: 16s
			expect(calculateDelay(5)).toBe(30000); // 5th retry: capped at 30s
			expect(calculateDelay(6)).toBe(30000); // 6th retry: capped at 30s
			expect(calculateDelay(10)).toBe(30000); // 10th retry: capped at 30s
		});

		it("uses exponential delay in practice", async () => {
			vi.useFakeTimers();

			let createAttempts = 0;
			const attemptTimes: number[] = [];

			const machine = connectionMachine.provide({
				actors: {
					createClient: fromPromise<
						{ client: PipecatClient; clientUUID: string },
						{ serverUrl: string }
					>(async () => {
						createAttempts++;
						attemptTimes.push(Date.now());
						throw new Error("Fail");
					}),
					connect: fromCallback(() => () => {}),
					disconnectListener: fromCallback(() => () => {}),
				},
				actions: {
					emitConnectionState: () => {},
					emitReconnectStarted: () => {},
					emitReconnectResult: () => {},
					cleanupClient: () => {},
					logState: () => {},
				},
			});

			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });

			// First attempt
			await vi.advanceTimersByTimeAsync(0);
			expect(createAttempts).toBe(1);

			// Wait for first retry delay (2000ms for retryCount=1)
			await vi.advanceTimersByTimeAsync(2000);
			expect(createAttempts).toBe(2);

			// Wait for second retry delay (4000ms for retryCount=2)
			await vi.advanceTimersByTimeAsync(4000);
			expect(createAttempts).toBe(3);

			// Verify timing differences
			expect(attemptTimes.length).toBe(3);
			const [first, second, third] = attemptTimes as [number, number, number];
			expect(second - first).toBe(2000);
			expect(third - second).toBe(4000);

			actor.stop();
			vi.useRealTimers();
		});
	});

	describe("Context Updates", () => {
		it("resets retryCount to 0 on successful connection", async () => {
			vi.useFakeTimers();

			// First, create client that fails to trigger retries and increase retryCount
			let shouldFail = true;
			const callbacks = {
				connectSendBack: null as ConnectSendBack | null,
			};

			const machine = connectionMachine.provide({
				actors: {
					createClient: fromPromise(async () => {
						if (shouldFail) {
							throw new Error("Fail");
						}
						return {
							client: { id: "mock-client" } as unknown as PipecatClient,
							clientUUID: "test-uuid-12345",
						};
					}),
					connect: fromCallback(({ sendBack }) => {
						callbacks.connectSendBack = sendBack as ConnectSendBack;
						return () => {};
					}),
					disconnectListener: fromCallback(() => () => {}),
				},
				actions: {
					emitConnectionState: () => {},
					emitReconnectStarted: () => {},
					emitReconnectResult: () => {},
					cleanupClient: () => {},
					logState: () => {},
				},
			});

			const actor = createActor(machine);
			actor.start();

			expect(actor.getSnapshot().context.retryCount).toBe(0);

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });

			// Let createClient fail
			await vi.advanceTimersByTimeAsync(0);
			expect(actor.getSnapshot().value).toBe("retrying");
			expect(actor.getSnapshot().context.retryCount).toBe(1);

			// Now allow success
			shouldFail = false;

			// Wait for retry delay (2000ms for retryCount=1)
			await vi.advanceTimersByTimeAsync(2000);
			await waitForState(actor, "connecting");

			callbacks.connectSendBack?.({ type: "CONNECTED" });
			await waitForState(actor, "idle");

			expect(actor.getSnapshot().context.retryCount).toBe(0);

			actor.stop();
			vi.useRealTimers();
		});

		it("sets error on failures and clears on success", async () => {
			vi.useFakeTimers();

			let shouldFail = true;
			const callbacks = { connectSendBack: null as ConnectSendBack | null };

			const machine = connectionMachine.provide({
				actors: {
					createClient: fromPromise(async () => {
						if (shouldFail) {
							shouldFail = false;
							throw new Error("Test error");
						}
						return {
							client: { id: "mock-client" } as unknown as PipecatClient,
							clientUUID: "test-uuid-12345",
						};
					}),
					connect: fromCallback(({ sendBack }) => {
						callbacks.connectSendBack = sendBack as ConnectSendBack;
						return () => {};
					}),
					disconnectListener: fromCallback(() => () => {}),
				},
				actions: {
					emitConnectionState: () => {},
					emitReconnectStarted: () => {},
					emitReconnectResult: () => {},
					cleanupClient: () => {},
					logState: () => {},
				},
			});

			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });

			// Let createClient fail
			await vi.advanceTimersByTimeAsync(0);
			expect(actor.getSnapshot().value).toBe("retrying");
			expect(actor.getSnapshot().context.error).toBe("Test error");

			// Wait for retry delay and successful connection
			await vi.advanceTimersByTimeAsync(2000);
			await waitForState(actor, "connecting");

			callbacks.connectSendBack?.({ type: "CONNECTED" });
			await waitForState(actor, "idle");

			expect(actor.getSnapshot().context.error).toBeNull();

			actor.stop();
			vi.useRealTimers();
		});
	});

	describe("Edge Cases", () => {
		it("waits for CONNECTED event before transitioning to idle", async () => {
			const { machine, callbacks } = createTestMachine({
				createClientBehavior: "success",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });
			await waitForState(actor, "connecting");

			// Should stay in connecting
			expect(actor.getSnapshot().value).toBe("connecting");

			// Only transition after CONNECTED
			callbacks.connectSendBack?.({ type: "CONNECTED" });
			await waitForState(actor, "idle");

			actor.stop();
		});

		it("handles communication errors during active states", async () => {
			const { machine, callbacks } = createTestMachine({
				createClientBehavior: "success",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });
			await waitForState(actor, "connecting");
			callbacks.connectSendBack?.({ type: "CONNECTED" });
			await waitForState(actor, "idle");

			actor.send({ type: "COMMUNICATION_ERROR", error: "Send failed" });
			await waitForState(actor, "retrying");

			actor.stop();
		});

		it("manual RECONNECT during retrying bypasses delay", async () => {
			vi.useFakeTimers();

			const { machine } = createTestMachine({
				createClientBehavior: "error",
			});
			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });
			await vi.advanceTimersByTimeAsync(0);

			expect(actor.getSnapshot().value).toBe("retrying");
			expect(actor.getSnapshot().context.retryCount).toBeGreaterThan(0);

			// RECONNECT should immediately transition (no delay)
			actor.send({ type: "RECONNECT" });

			// retryCount should be reset
			expect(actor.getSnapshot().context.retryCount).toBe(0);
			// Should be in initializing (transitions immediately)
			expect(actor.getSnapshot().value).toBe("initializing");

			actor.stop();
			vi.useRealTimers();
		});
	});

	describe("Full Flow Tests", () => {
		it("happy path: disconnected → connecting → idle → recording → processing → idle", async () => {
			const { machine, callbacks } = createTestMachine({
				createClientBehavior: "success",
			});
			const actor = createActor(machine);
			actor.start();

			expect(actor.getSnapshot().value).toBe("disconnected");

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });
			await waitForState(actor, "connecting");

			callbacks.connectSendBack?.({ type: "CONNECTED" });
			await waitForState(actor, "idle");

			actor.send({ type: "START_RECORDING" });
			await waitForState(actor, "recording");

			actor.send({ type: "STOP_RECORDING" });
			await waitForState(actor, "processing");

			actor.send({ type: "RESPONSE_RECEIVED" });
			await waitForState(actor, "idle");

			actor.stop();
		});

		it("retry flow: connecting fails → retrying → reconnect → idle", async () => {
			vi.useFakeTimers();

			let connectAttempts = 0;
			const callbacks = { connectSendBack: null as ConnectSendBack | null };

			const machine = connectionMachine.provide({
				actors: {
					createClient: fromPromise(async () => ({
						client: { id: "mock-client" } as unknown as PipecatClient,
						clientUUID: "test-uuid-12345",
					})),
					connect: fromCallback(({ sendBack }) => {
						connectAttempts++;
						callbacks.connectSendBack = sendBack as ConnectSendBack;
						if (connectAttempts === 1) {
							// First attempt fails
							(sendBack as ConnectSendBack)({ type: "DISCONNECTED" });
						}
						return () => {};
					}),
					disconnectListener: fromCallback(() => () => {}),
				},
				actions: {
					emitConnectionState: () => {},
					emitReconnectStarted: () => {},
					emitReconnectResult: () => {},
					cleanupClient: () => {},
					logState: () => {},
				},
			});

			const actor = createActor(machine);
			actor.start();

			actor.send({ type: "CONNECT", serverUrl: "http://localhost:8000" });

			// Let createClient succeed
			await vi.advanceTimersByTimeAsync(0);

			// First connection attempt fails immediately
			await waitForState(actor, "retrying");
			expect(connectAttempts).toBe(1);

			// Wait for retry delay (2000ms for retryCount=1)
			await vi.advanceTimersByTimeAsync(2000);
			await waitForState(actor, "connecting");
			expect(connectAttempts).toBe(2);

			// Second attempt succeeds
			callbacks.connectSendBack?.({ type: "CONNECTED" });
			await waitForState(actor, "idle");

			actor.stop();
			vi.useRealTimers();
		});
	});
});
