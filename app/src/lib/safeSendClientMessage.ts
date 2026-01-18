import type { PipecatClient } from "@pipecat-ai/client-js";
import type { SmallWebRTCTransport } from "@pipecat-ai/small-webrtc-transport";

type SendResult =
	| { success: true }
	| { success: false; reason: "not_ready" | "send_failed"; error?: string };

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
	// Check transport state before sending
	const transport = client.transport as SmallWebRTCTransport;
	if (transport.state !== "ready") {
		const error = `Transport not ready: ${transport.state}`;
		console.warn(`[safeSend] ${error}`);
		onCommunicationError?.(error);
		return { success: false, reason: "not_ready" };
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
