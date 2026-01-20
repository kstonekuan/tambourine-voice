import { describe, expect, it } from "vitest";
import {
	type HotkeyConfig,
	hotkeyIsSameAs,
	validateHotkeyNotDuplicate,
} from "./tauri";

describe("hotkeyIsSameAs", () => {
	it("returns true for identical hotkeys", () => {
		const a: HotkeyConfig = {
			modifiers: ["ctrl", "alt"],
			key: "Space",
			enabled: true,
		};
		const b: HotkeyConfig = {
			modifiers: ["ctrl", "alt"],
			key: "Space",
			enabled: true,
		};
		expect(hotkeyIsSameAs(a, b)).toBe(true);
	});

	it("is case-insensitive for keys", () => {
		const a: HotkeyConfig = {
			modifiers: ["ctrl"],
			key: "space",
			enabled: true,
		};
		const b: HotkeyConfig = {
			modifiers: ["ctrl"],
			key: "SPACE",
			enabled: true,
		};
		expect(hotkeyIsSameAs(a, b)).toBe(true);
	});

	it("is case-insensitive for modifiers", () => {
		const a: HotkeyConfig = {
			modifiers: ["CTRL", "ALT"],
			key: "Space",
			enabled: true,
		};
		const b: HotkeyConfig = {
			modifiers: ["ctrl", "alt"],
			key: "Space",
			enabled: true,
		};
		expect(hotkeyIsSameAs(a, b)).toBe(true);
	});

	it("returns true for modifiers in different order", () => {
		const a: HotkeyConfig = {
			modifiers: ["ctrl", "alt"],
			key: "Space",
			enabled: true,
		};
		const b: HotkeyConfig = {
			modifiers: ["alt", "ctrl"],
			key: "Space",
			enabled: true,
		};
		expect(hotkeyIsSameAs(a, b)).toBe(true);
	});

	it("returns false for different keys", () => {
		const a: HotkeyConfig = {
			modifiers: ["ctrl"],
			key: "Space",
			enabled: true,
		};
		const b: HotkeyConfig = {
			modifiers: ["ctrl"],
			key: "Enter",
			enabled: true,
		};
		expect(hotkeyIsSameAs(a, b)).toBe(false);
	});

	it("returns false for different modifiers", () => {
		const a: HotkeyConfig = {
			modifiers: ["ctrl"],
			key: "Space",
			enabled: true,
		};
		const b: HotkeyConfig = { modifiers: ["alt"], key: "Space", enabled: true };
		expect(hotkeyIsSameAs(a, b)).toBe(false);
	});

	it("returns false for different modifier counts", () => {
		const a: HotkeyConfig = {
			modifiers: ["ctrl", "alt"],
			key: "Space",
			enabled: true,
		};
		const b: HotkeyConfig = {
			modifiers: ["ctrl"],
			key: "Space",
			enabled: true,
		};
		expect(hotkeyIsSameAs(a, b)).toBe(false);
	});
});

describe("validateHotkeyNotDuplicate", () => {
	const allHotkeys = {
		toggle: { modifiers: ["ctrl", "alt"], key: "Space", enabled: true },
		hold: { modifiers: ["ctrl", "alt"], key: "Backquote", enabled: true },
		paste_last: { modifiers: ["ctrl", "alt"], key: "Period", enabled: true },
	};

	it("returns null for a unique hotkey", () => {
		const result = validateHotkeyNotDuplicate(
			{ modifiers: ["ctrl", "shift"], key: "A", enabled: true },
			allHotkeys,
			"toggle",
		);
		expect(result).toBeNull();
	});

	it("returns null when using the same hotkey for the excluded type", () => {
		const result = validateHotkeyNotDuplicate(
			{ modifiers: ["ctrl", "alt"], key: "Space", enabled: true },
			allHotkeys,
			"toggle",
		);
		expect(result).toBeNull();
	});

	it("returns error message for duplicate hotkey", () => {
		const result = validateHotkeyNotDuplicate(
			{ modifiers: ["ctrl", "alt"], key: "Backquote", enabled: true },
			allHotkeys,
			"toggle",
		);
		expect(result).toBe("This shortcut is already used for the hold hotkey");
	});

	it("detects case-insensitive duplicates", () => {
		const result = validateHotkeyNotDuplicate(
			{ modifiers: ["CTRL", "ALT"], key: "BACKQUOTE", enabled: true },
			allHotkeys,
			"toggle",
		);
		expect(result).toBe("This shortcut is already used for the hold hotkey");
	});

	it("returns error message for duplicate with paste_last", () => {
		const result = validateHotkeyNotDuplicate(
			{ modifiers: ["ctrl", "alt"], key: "Period", enabled: true },
			allHotkeys,
			"hold",
		);
		expect(result).toBe(
			"This shortcut is already used for the paste last hotkey",
		);
	});
});
