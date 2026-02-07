import { Kbd, Switch, Tooltip } from "@mantine/core";
import { AlertCircle } from "lucide-react";
import { useEffect } from "react";
import { useRecordHotkeys } from "react-hotkeys-hook";
import type { HotkeyConfig } from "../lib/tauri";
import {
	type MutationStatus,
	StatusIndicator,
} from "./settings/StatusIndicator";

interface HotkeyInputProps {
	label: string;
	description?: string;
	value: HotkeyConfig;
	onChange: (config: HotkeyConfig) => void;
	disabled?: boolean;
	// Coordinated recording state (managed by parent)
	isRecording?: boolean;
	onStartRecording?: () => void;
	onStopRecording?: () => void;
	// Enable/disable toggle
	enabled?: boolean;
	onEnabledChange?: (enabled: boolean) => void;
	enabledLoading?: boolean;
	// Registration error (if hotkey couldn't be registered)
	registrationError?: string | null;
	// Mutation status for visual feedback
	mutationStatus?: MutationStatus;
}

// Known modifier keys (lowercase, as returned by react-hotkeys-hook)
const MODIFIER_KEYS = new Set(["ctrl", "alt", "shift", "meta", "mod"]);

/**
 * Map from react-hotkeys-hook key names to Tauri shortcut key names.
 * react-hotkeys-hook returns lowercase keys, Tauri expects specific formats.
 */
const KEY_NAME_MAP: Record<string, string> = {
	// Punctuation and special characters
	".": "Period",
	",": "Comma",
	"/": "Slash",
	"\\": "Backslash",
	";": "Semicolon",
	"'": "Quote",
	"[": "BracketLeft",
	"]": "BracketRight",
	"`": "Backquote",
	"-": "Minus",
	"=": "Equal",
	// Named keys (react-hotkeys-hook returns these in lowercase)
	space: "Space",
	backspace: "Backspace",
	tab: "Tab",
	enter: "Enter",
	escape: "Escape",
	delete: "Delete",
	insert: "Insert",
	home: "Home",
	end: "End",
	pageup: "PageUp",
	pagedown: "PageDown",
	// Arrow keys
	arrowup: "ArrowUp",
	arrowdown: "ArrowDown",
	arrowleft: "ArrowLeft",
	arrowright: "ArrowRight",
	up: "ArrowUp",
	down: "ArrowDown",
	left: "ArrowLeft",
	right: "ArrowRight",
	// Function keys
	f1: "F1",
	f2: "F2",
	f3: "F3",
	f4: "F4",
	f5: "F5",
	f6: "F6",
	f7: "F7",
	f8: "F8",
	f9: "F9",
	f10: "F10",
	f11: "F11",
	f12: "F12",
	// Numpad
	numpad0: "Numpad0",
	numpad1: "Numpad1",
	numpad2: "Numpad2",
	numpad3: "Numpad3",
	numpad4: "Numpad4",
	numpad5: "Numpad5",
	numpad6: "Numpad6",
	numpad7: "Numpad7",
	numpad8: "Numpad8",
	numpad9: "Numpad9",
	numpadadd: "NumpadAdd",
	numpadsubtract: "NumpadSubtract",
	numpadmultiply: "NumpadMultiply",
	numpaddivide: "NumpadDivide",
	numpaddecimal: "NumpadDecimal",
	numpadenter: "NumpadEnter",
	// Special named keys that might come through
	backquote: "Backquote",
	period: "Period",
	comma: "Comma",
	slash: "Slash",
	semicolon: "Semicolon",
	quote: "Quote",
	bracketleft: "BracketLeft",
	bracketright: "BracketRight",
	backslash: "Backslash",
	minus: "Minus",
	equal: "Equal",
};

/**
 * Convert a key from react-hotkeys-hook format to Tauri format
 */
function formatKeyForTauri(key: string): string {
	// Check if we have an explicit mapping
	const mapped = KEY_NAME_MAP[key.toLowerCase()];
	if (mapped) {
		return mapped;
	}

	// For single letters/numbers, uppercase them
	if (key.length === 1) {
		return key.toUpperCase();
	}

	// For other keys, capitalize first letter of each word (e.g., "capslock" -> "CapsLock")
	return key
		.split(/(?=[A-Z])|[-_]/)
		.map((part) => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
		.join("");
}

/**
 * Convert recorded keys Set to HotkeyConfig
 */
function keysToConfig(
	keys: Set<string>,
	currentEnabled: boolean,
): HotkeyConfig | null {
	const keysArray = Array.from(keys);
	const modifiers: string[] = [];
	let mainKey: string | null = null;

	for (const key of keysArray) {
		if (MODIFIER_KEYS.has(key.toLowerCase())) {
			modifiers.push(key.toLowerCase());
		} else {
			// The non-modifier key (should only be one)
			mainKey = key;
		}
	}

	// Require at least one modifier and exactly one main key
	if (modifiers.length === 0 || mainKey === null) {
		return null;
	}

	return {
		modifiers,
		key: formatKeyForTauri(mainKey),
		enabled: currentEnabled, // Preserve current enabled state
	};
}

/**
 * Format a key for display (e.g., "ctrl" -> "Ctrl", "Space" -> "Space")
 */
function formatKeyForDisplay(key: string): string {
	return key.charAt(0).toUpperCase() + key.slice(1);
}

export function HotkeyInput({
	label,
	description,
	value,
	onChange,
	disabled,
	isRecording: externalIsRecording,
	onStartRecording,
	onStopRecording,
	enabled,
	onEnabledChange,
	enabledLoading,
	registrationError,
	mutationStatus,
}: HotkeyInputProps) {
	const [keys, { start, stop, isRecording: internalIsRecording }] =
		useRecordHotkeys();

	// Use external state if provided, otherwise use internal
	const isRecording = externalIsRecording ?? internalIsRecording;

	// Handle Escape key to cancel recording
	useEffect(() => {
		if (!isRecording) return;

		const handleEscape = (event: KeyboardEvent) => {
			if (event.key === "Escape") {
				event.preventDefault();
				stop();
				onStopRecording?.();
			}
		};

		document.addEventListener("keydown", handleEscape);
		return () => document.removeEventListener("keydown", handleEscape);
	}, [isRecording, stop, onStopRecording]);

	// Watch for key changes and update when we have a valid combination
	useEffect(() => {
		if (!isRecording) return;
		if (keys.size === 0) return;

		// Check if Escape was pressed (handled separately)
		if (keys.has("escape")) {
			return;
		}

		const config = keysToConfig(keys, value.enabled);
		if (config) {
			onChange(config);
			stop();
			onStopRecording?.();
		}
	}, [keys, isRecording, onChange, stop, onStopRecording, value.enabled]);

	// Sync internal recording state with external state
	useEffect(() => {
		if (externalIsRecording === true && !internalIsRecording) {
			start();
		} else if (externalIsRecording === false && internalIsRecording) {
			stop();
		}
	}, [externalIsRecording, internalIsRecording, start, stop]);

	const handleClick = () => {
		if (disabled) return;
		// Allow changing hotkey even when disabled (enabled=false)
		// so user can fix conflicts

		if (isRecording) {
			// Clicking again cancels
			stop();
			onStopRecording?.();
		} else {
			start();
			onStartRecording?.();
		}
	};

	// Build live preview of current keys being pressed
	const livePreview = Array.from(keys)
		.filter((k) => k !== "escape")
		.map((k) => formatKeyForDisplay(k));

	// Determine if the toggle should be disabled
	// Allow enabling even with registration error (user may have fixed conflict externally)
	const toggleDisabled = disabled || enabledLoading;

	return (
		<div>
			<div
				style={{
					display: "flex",
					alignItems: "center",
					justifyContent: "space-between",
					gap: 12,
				}}
			>
				<div style={{ flex: 1 }}>
					<div style={{ display: "flex", alignItems: "center", gap: 8 }}>
						<p className="settings-label" style={{ margin: 0 }}>
							{label}
						</p>
						{mutationStatus && <StatusIndicator status={mutationStatus} />}
						{registrationError && (
							<Tooltip
								label={registrationError}
								multiline
								w={250}
								withArrow
								position="top"
							>
								<AlertCircle
									size={16}
									style={{ color: "var(--mantine-color-yellow-6)" }}
								/>
							</Tooltip>
						)}
					</div>
					{description && <p className="settings-description">{description}</p>}
				</div>
				{onEnabledChange !== undefined && (
					<Tooltip
						label={
							registrationError && !enabled
								? "Try enabling (may fail if conflict still exists)"
								: enabled
									? "Disable this hotkey"
									: "Enable this hotkey"
						}
						position="left"
						withArrow
					>
						<Switch
							checked={enabled ?? true}
							onChange={(e) => onEnabledChange(e.currentTarget.checked)}
							disabled={toggleDisabled}
							size="md"
						/>
					</Tooltip>
				)}
			</div>
			<button
				type="button"
				onClick={handleClick}
				disabled={disabled}
				className={`hotkey-display ${isRecording ? "capturing" : ""}`}
				aria-label={isRecording ? "Recording shortcut; Press keys or Escape to cancel" : "Click to record a new shortcut"}
				aria-pressed={isRecording}
				style={{
					width: "100%",
					marginTop: 8,
					cursor: disabled ? "not-allowed" : "pointer",
					opacity: disabled ? 0.5 : 1,
				}}
			>
				{isRecording ? (
					<div
						style={{
							display: "flex",
							alignItems: "center",
							justifyContent: "center",
							gap: 8,
						}}
					>
						{livePreview.length > 0 ? (
							<>
								{livePreview.map((part) => (
									<Kbd key={part}>{part}</Kbd>
								))}
								<span
									style={{
										color: "var(--text-secondary)",
										fontSize: 12,
										marginLeft: 4,
									}}
								>
									+ key
								</span>
							</>
						) : (
							<span style={{ color: "var(--accent-primary)", fontSize: 14 }}>
								Press a key combination...
							</span>
						)}
						<span
							style={{
								color: "var(--text-tertiary)",
								fontSize: 11,
								marginLeft: 8,
							}}
						>
							(Esc to cancel)
						</span>
					</div>
				) : (
					<>
						{value.modifiers.concat([value.key]).map((part) => (
							<Kbd key={part}>{formatKeyForDisplay(part)}</Kbd>
						))}
						<span className="hotkey-hint">Click to change</span>
					</>
				)}
			</button>
		</div>
	);
}
