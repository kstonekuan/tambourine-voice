import type { PipecatClient } from "@pipecat-ai/client-js";
import { describe, expect, it, vi } from "vitest";
import { safeSendClientMessage } from "./safeSendClientMessage";

function createClientDouble(options: {
	transportState?: string;
	throwOnSend?: boolean;
}) {
	const { transportState, throwOnSend = false } = options;
	const sendClientMessage = vi.fn(() => {
		if (throwOnSend) {
			throw new Error("network unavailable");
		}
	});

	const client = {
		transport:
			typeof transportState === "string"
				? { state: transportState }
				: undefined,
		sendClientMessage,
	} as unknown as PipecatClient;

	return { client, sendClientMessage };
}

describe("safeSendClientMessage", () => {
	it("returns success when transport is ready and send succeeds", () => {
		const { client, sendClientMessage } = createClientDouble({
			transportState: "ready",
		});

		const result = safeSendClientMessage(client, "start-recording", {});

		expect(result).toEqual({ success: true });
		expect(sendClientMessage).toHaveBeenCalledWith("start-recording", {});
	});

	it("returns not_ready when transport state is not ready", () => {
		const { client, sendClientMessage } = createClientDouble({
			transportState: "connecting",
		});

		const result = safeSendClientMessage(client, "start-recording", {});

		expect(result).toEqual({
			success: false,
			reason: "not_ready",
			error: "Transport not ready: connecting",
		});
		expect(sendClientMessage).not.toHaveBeenCalled();
	});

	it("returns not_ready instead of throwing when transport is unavailable", () => {
		const { client, sendClientMessage } = createClientDouble({});
		const onCommunicationError = vi.fn();

		const result = safeSendClientMessage(
			client,
			"start-recording",
			{},
			onCommunicationError,
		);

		expect(result).toEqual({
			success: false,
			reason: "not_ready",
			error: "Transport not available",
		});
		expect(onCommunicationError).toHaveBeenCalledWith(
			"Transport not available",
		);
		expect(sendClientMessage).not.toHaveBeenCalled();
	});

	it("returns send_failed when underlying send throws", () => {
		const { client } = createClientDouble({
			transportState: "ready",
			throwOnSend: true,
		});

		const result = safeSendClientMessage(client, "stop-recording", {});

		expect(result).toEqual({
			success: false,
			reason: "send_failed",
			error: "network unavailable",
		});
	});
});
