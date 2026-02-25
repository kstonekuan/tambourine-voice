import { afterEach, describe, expect, it, vi } from "vitest";
import { formatKeyForPlatform } from "./platformKeys";

vi.mock("./runtimePlatform", () => ({
	isMacOSRuntime: vi.fn(),
}));

import { isMacOSRuntime } from "./runtimePlatform";

const mockIsMacOSRuntime = vi.mocked(isMacOSRuntime);

afterEach(() => {
	vi.resetAllMocks();
});

describe("formatKeyForPlatform", () => {
	describe("on macOS", () => {
		const EXPECTED_MAC_CTRL = "⌃";
		const EXPECTED_MAC_ALT = "⌥";
		const EXPECTED_MAC_SHIFT = "⇧";
		const EXPECTED_MAC_META = "⌘";

		it("maps ctrl to Control symbol", () => {
			mockIsMacOSRuntime.mockReturnValue(true);
			expect(formatKeyForPlatform("ctrl")).toBe(EXPECTED_MAC_CTRL);
		});

		it("maps alt to Option symbol", () => {
			mockIsMacOSRuntime.mockReturnValue(true);
			expect(formatKeyForPlatform("alt")).toBe(EXPECTED_MAC_ALT);
		});

		it("maps shift to Shift symbol", () => {
			mockIsMacOSRuntime.mockReturnValue(true);
			expect(formatKeyForPlatform("shift")).toBe(EXPECTED_MAC_SHIFT);
		});

		it("maps meta to Command symbol", () => {
			mockIsMacOSRuntime.mockReturnValue(true);
			expect(formatKeyForPlatform("meta")).toBe(EXPECTED_MAC_META);
		});

		it("is case-insensitive for modifiers", () => {
			mockIsMacOSRuntime.mockReturnValue(true);
			expect(formatKeyForPlatform("Ctrl")).toBe(EXPECTED_MAC_CTRL);
			expect(formatKeyForPlatform("ALT")).toBe(EXPECTED_MAC_ALT);
		});

		it("capitalizes non-modifier keys", () => {
			mockIsMacOSRuntime.mockReturnValue(true);
			expect(formatKeyForPlatform("Space")).toBe("Space");
			expect(formatKeyForPlatform("backquote")).toBe("Backquote");
		});
	});

	describe("on non-macOS", () => {
		it("capitalizes modifier keys", () => {
			mockIsMacOSRuntime.mockReturnValue(false);
			expect(formatKeyForPlatform("ctrl")).toBe("Ctrl");
			expect(formatKeyForPlatform("alt")).toBe("Alt");
			expect(formatKeyForPlatform("shift")).toBe("Shift");
			expect(formatKeyForPlatform("meta")).toBe("Meta");
		});

		it("capitalizes non-modifier keys", () => {
			mockIsMacOSRuntime.mockReturnValue(false);
			expect(formatKeyForPlatform("Space")).toBe("Space");
			expect(formatKeyForPlatform("period")).toBe("Period");
		});
	});
});
