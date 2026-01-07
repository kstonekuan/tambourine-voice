import { describe, expect, it } from "vitest";
import {
	createHotkeyDuplicateSchema,
	type HotkeyConfig,
	HotkeyConfigSchema,
	hotkeyIsSameAs,
	validateHotkeyNotDuplicate,
} from "./tauri";

describe("HotkeyConfigSchema", () => {
	it("validates a valid hotkey config", () => {
		const result = HotkeyConfigSchema.safeParse({
			modifiers: ["ctrl", "alt"],
			key: "Space",
		});
		expect(result.success).toBe(true);
	});

	it("rejects empty key", () => {
		const result = HotkeyConfigSchema.safeParse({
			modifiers: ["ctrl"],
			key: "",
		});
		expect(result.success).toBe(false);
	});

	it("accepts empty modifiers", () => {
		const result = HotkeyConfigSchema.safeParse({
			modifiers: [],
			key: "F1",
		});
		expect(result.success).toBe(true);
	});

	it("rejects non-string modifiers", () => {
		const result = HotkeyConfigSchema.safeParse({
			modifiers: [123],
			key: "Space",
		});
		expect(result.success).toBe(false);
	});
});

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

describe("createHotkeyDuplicateSchema", () => {
	const allHotkeys = {
		toggle: { modifiers: ["ctrl", "alt"], key: "Space", enabled: true },
		hold: { modifiers: ["ctrl", "alt"], key: "Backquote", enabled: true },
		paste_last: { modifiers: ["ctrl", "alt"], key: "Period", enabled: true },
	};

	it("allows a unique hotkey when editing toggle", () => {
		const schema = createHotkeyDuplicateSchema(allHotkeys, "toggle");
		const result = schema.safeParse({
			modifiers: ["ctrl", "shift"],
			key: "A",
		});
		expect(result.success).toBe(true);
	});

	it("allows the same hotkey as the excluded type", () => {
		const schema = createHotkeyDuplicateSchema(allHotkeys, "toggle");
		// Editing toggle, so we can use the current toggle hotkey
		const result = schema.safeParse({
			modifiers: ["ctrl", "alt"],
			key: "Space",
		});
		expect(result.success).toBe(true);
	});

	it("rejects a hotkey that conflicts with another type", () => {
		const schema = createHotkeyDuplicateSchema(allHotkeys, "toggle");
		// Trying to use hold's hotkey for toggle
		const result = schema.safeParse({
			modifiers: ["ctrl", "alt"],
			key: "Backquote",
		});
		expect(result.success).toBe(false);
		if (!result.success) {
			expect(result.error.issues[0]?.message).toContain("hold");
		}
	});

	it("rejects a hotkey that conflicts with paste_last", () => {
		const schema = createHotkeyDuplicateSchema(allHotkeys, "hold");
		const result = schema.safeParse({
			modifiers: ["ctrl", "alt"],
			key: "Period",
		});
		expect(result.success).toBe(false);
		if (!result.success) {
			expect(result.error.issues[0]?.message).toContain("paste last");
		}
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
});
