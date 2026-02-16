import type { PipecatClient } from "@pipecat-ai/client-js";
import type { SmallWebRTCTransport } from "@pipecat-ai/small-webrtc-transport";
import { match, P } from "ts-pattern";
import type { LLMProviderSelection, STTProviderSelection } from "./tauri";

// Discriminated union for type-safe config messages
// Only provider switching uses RTVI (requires frame injection into pipeline)
// Prompt sections and STT timeout now use HTTP API
export type ConfigMessage =
	| { type: "set-stt-provider"; data: { provider: STTProviderSelection } }
	| { type: "set-llm-provider"; data: { provider: LLMProviderSelection } };

export type SendResultSuccess = { success: true };
export type SendResultNotReady = {
	success: false;
	reason: "not_ready";
	error?: string;
};
export type SendResultSendFailed = {
	success: false;
	reason: "send_failed";
	error: string;
};

export type SendResult =
	| SendResultSuccess
	| SendResultNotReady
	| SendResultSendFailed;

/**
 * Pattern match on SendResult with exhaustive checking.
 * Ensures all result variants are handled at compile time.
 *
 * @example
 * matchSendResult(result, {
 *   onSuccess: () => console.log("Message sent"),
 *   onNotReady: (error) => console.warn("Transport not ready:", error),
 *   onSendFailed: (error) => console.error("Send failed:", error),
 * });
 */
export function matchSendResult<T>(
	result: SendResult,
	handlers: {
		onSuccess: () => T;
		onNotReady: (error: string | undefined) => T;
		onSendFailed: (error: string) => T;
	},
): T {
	return match(result)
		.with({ success: true }, () => handlers.onSuccess())
		.with({ success: false, reason: "not_ready" }, (r) =>
			handlers.onNotReady(r.error),
		)
		.with({ success: false, reason: "send_failed" }, (r) =>
			handlers.onSendFailed(r.error),
		)
		.exhaustive();
}

/**
 * Safely sends a message through the PipecatClient, detecting failures
 * that should trigger reconnection.
 *
 * This wrapper provides:
 * - Pre-flight check: Verifies transport is in "ready" state before sending
 * - Exception handling: Catches and reports send failures
 * - Reconnection trigger: Calls onCommunicationError callback for recovery
 *
 * Use this for critical messages (start-recording, stop-recording) where
 * silent failure would leave the app in an inconsistent state.
 */
export function safeSendClientMessage(
	client: PipecatClient,
	messageType: string,
	data: unknown,
	onCommunicationError?: (error: string) => void,
): SendResult {
	const maybeTransport = client.transport as
		| SmallWebRTCTransport
		| undefined
		| null;

	const transportNotReadyResult = match(maybeTransport)
		.with(P.nullish, () => {
			const error = "Transport not available";
			return { success: false, reason: "not_ready", error } as const;
		})
		.with({ state: "ready" }, () => null)
		.with({ state: P.string }, (transport) => {
			const error = `Transport not ready: ${transport.state}`;
			return { success: false, reason: "not_ready", error } as const;
		})
		.otherwise(() => {
			const error = "Transport not available";
			return { success: false, reason: "not_ready", error } as const;
		});

	if (transportNotReadyResult !== null) {
		console.warn(`[safeSend] ${transportNotReadyResult.error}`);
		onCommunicationError?.(transportNotReadyResult.error);
		return transportNotReadyResult;
	}

	try {
		client.sendClientMessage(messageType, data);
		return { success: true };
	} catch (e) {
		const error = e instanceof Error ? e.message : String(e);
		console.warn(`[safeSend] Send failed: ${error}`);
		onCommunicationError?.(error);
		return { success: false, reason: "send_failed", error };
	}
}

/**
 * Send config messages to the server via RTVI.
 * Stops on first failure to allow reconnection to handle re-syncing.
 */
export function sendConfigMessages(
	client: PipecatClient,
	messages: ConfigMessage[],
	onCommunicationError?: (error: string) => void,
): void {
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
