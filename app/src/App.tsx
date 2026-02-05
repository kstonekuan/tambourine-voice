import { Kbd, Loader, NavLink, Text, Title, Tooltip } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { AlertCircle, Home, Settings } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { match } from "ts-pattern";
import { HistoryFeed } from "./components/HistoryFeed";
import { Logo } from "./components/Logo";
import { AudioSettings } from "./components/settings/AudioSettings";
import { ConnectionSettings } from "./components/settings/ConnectionSettings";
import { DataManagementSettings } from "./components/settings/DataManagementSettings";
import { HotkeySettings } from "./components/settings/HotkeySettings";
import { PromptSettings } from "./components/settings/PromptSettings";
import { ProvidersSettings } from "./components/settings/ProvidersSettings";
import {
	DEFAULT_HOLD_HOTKEY,
	DEFAULT_PASTE_LAST_HOTKEY,
	DEFAULT_TOGGLE_HOTKEY,
} from "./lib/hotkeyDefaults";
import {
	useRefreshServerQueriesOnConnect,
	useSettings,
	useShortcutErrors,
} from "./lib/queries";
import {
	type ConfigResponse,
	type HotkeyConfig,
	type LLMErrorPayload,
	tauriAPI,
} from "./lib/tauri";
import { useRecordingStore } from "./stores/recordingStore";
import "./app-main.css";

type View = "home" | "settings";

function ConnectionStatusIndicator() {
	const state = useRecordingStore((s) => s.state);
	const setState = useRecordingStore((s) => s.setState);

	// Listen for connection state changes from the overlay window
	useEffect(() => {
		let unlisten: (() => void) | undefined;

		const setup = async () => {
			unlisten = await tauriAPI.onConnectionStateChanged((newState) => {
				setState(newState);
			});
		};

		setup();

		return () => {
			unlisten?.();
		};
	}, [setState]);

	const isConnected =
		state === "idle" || state === "recording" || state === "processing";
	const isConnecting = state === "connecting";

	const statusText = match(state)
		.with("connecting", () => "Connecting...")
		.with("idle", "recording", "processing", () => "Connected")
		.otherwise(() => "Disconnected");

	return (
		<Tooltip label={statusText} position="right" withArrow>
			<div className="connection-status">
				{isConnecting ? (
					<Loader size={10} color="gray" />
				) : (
					<span
						className={`connection-status-dot ${isConnected ? "connected" : "disconnected"}`}
					/>
				)}
			</div>
		</Tooltip>
	);
}

function Sidebar({
	activeView,
	onViewChange,
}: {
	activeView: View;
	onViewChange: (view: View) => void;
}) {
	return (
		<aside className="sidebar">
			<header className="sidebar-header">
				<div className="sidebar-logo">
					<Logo size={32} />
				</div>
			</header>

			<nav className="sidebar-nav">
				<Tooltip label="Home" position="right" withArrow>
					<NavLink
						leftSection={<Home size={20} />}
						active={activeView === "home"}
						onClick={() => onViewChange("home")}
						variant="filled"
						className="sidebar-nav-link"
					/>
				</Tooltip>
				<Tooltip label="Settings" position="right" withArrow>
					<NavLink
						leftSection={<Settings size={20} />}
						active={activeView === "settings"}
						onClick={() => onViewChange("settings")}
						variant="filled"
						className="sidebar-nav-link"
					/>
				</Tooltip>
			</nav>

			<footer className="sidebar-footer">
				<ConnectionStatusIndicator />
			</footer>
		</aside>
	);
}

function HotkeyDisplay({
	config,
	error,
}: {
	config: HotkeyConfig;
	error?: string | null;
}) {
	const isDisabled = config.enabled === false;
	const parts = [
		...config.modifiers.map((m) => m.charAt(0).toUpperCase() + m.slice(1)),
		config.key,
	];

	return (
		<span
			className="kbd-combo"
			style={{
				display: "flex",
				alignItems: "center",
				gap: 6,
				opacity: isDisabled ? 0.5 : 1,
			}}
		>
			{error && (
				<Tooltip label={error} multiline w={250} withArrow position="top">
					<AlertCircle
						size={14}
						style={{ color: "var(--mantine-color-yellow-6)", flexShrink: 0 }}
					/>
				</Tooltip>
			)}
			{isDisabled && !error && (
				<span style={{ color: "var(--text-tertiary)", fontSize: 12 }}>
					(Disabled)
				</span>
			)}
			{parts.map((part, index) => (
				<span key={part}>
					<Kbd>{part}</Kbd>
					{index < parts.length - 1 && <span className="kbd-plus">+</span>}
				</span>
			))}
		</span>
	);
}

function InstructionsCard() {
	const { data: settings } = useSettings();
	const { data: shortcutErrors } = useShortcutErrors();

	const toggleHotkey = settings?.toggle_hotkey ?? DEFAULT_TOGGLE_HOTKEY;
	const holdHotkey = settings?.hold_hotkey ?? DEFAULT_HOLD_HOTKEY;
	const pasteLastHotkey =
		settings?.paste_last_hotkey ?? DEFAULT_PASTE_LAST_HOTKEY;

	return (
		<div className="instructions-card animate-in">
			<h2 className="instructions-card-title">Dictate with your voice</h2>
			<div className="instructions-methods">
				<div className="instruction-method">
					<span className="instruction-label">Toggle:</span>
					<HotkeyDisplay
						config={toggleHotkey}
						error={shortcutErrors?.toggle_error}
					/>
					<span className="instruction-desc">Press to start/stop</span>
				</div>
				<div className="instruction-method">
					<span className="instruction-label">Hold:</span>
					<HotkeyDisplay
						config={holdHotkey}
						error={shortcutErrors?.hold_error}
					/>
					<span className="instruction-desc">Hold to record</span>
				</div>
				<div className="instruction-method">
					<span className="instruction-label">Paste:</span>
					<HotkeyDisplay
						config={pasteLastHotkey}
						error={shortcutErrors?.paste_last_error}
					/>
					<span className="instruction-desc">Paste last result</span>
				</div>
			</div>
			<p className="instructions-card-text">
				Speak clearly and your words will be typed wherever your cursor is. The
				overlay appears in the bottom-right corner of your screen.
			</p>
		</div>
	);
}

function HomeView() {
	return (
		<div className="main-content">
			<header className="animate-in" style={{ marginBottom: 32 }}>
				<Title order={1} mb={4}>
					Welcome to Tambourine
				</Title>
				<Text c="dimmed" size="sm">
					~-~-~-~-~-~
				</Text>
			</header>

			<InstructionsCard />

			<HistoryFeed />
		</div>
	);
}

function SettingsView() {
	const connectionState = useRecordingStore((s) => s.state);
	const isRecordingActive =
		connectionState === "recording" || connectionState === "processing";

	return (
		<div
			style={{
				position: "relative",
				flex: 1,
				display: "flex",
				flexDirection: "column",
				minWidth: 0,
			}}
		>
			{isRecordingActive && (
				<div
					style={{
						position: "absolute",
						inset: 0,
						zIndex: 10,
						display: "flex",
						alignItems: "center",
						justifyContent: "center",
						backgroundColor: "rgba(0, 0, 0, 0.6)",
						backdropFilter: "blur(2px)",
					}}
				>
					<Text c="dimmed" size="sm">
						Settings are locked while recording
					</Text>
				</div>
			)}
			<div className="main-content">
				<header className="animate-in" style={{ marginBottom: 32 }}>
					<Title order={1} mb={4}>
						Settings
					</Title>
					<Text c="dimmed" size="sm">
						Configure your preferences
					</Text>
				</header>

				<ProvidersSettings />
				<AudioSettings />
				<HotkeySettings />
				<PromptSettings />
				<ConnectionSettings />
				<DataManagementSettings />
			</div>
		</div>
	);
}

// Type-safe setting name mapping
type SettingKey =
	| "stt-provider"
	| "llm-provider"
	| "prompt-sections"
	| "stt-timeout";

const SETTING_DISPLAY_NAMES: Record<SettingKey, string> = {
	"stt-provider": "STT provider",
	"llm-provider": "LLM provider",
	"prompt-sections": "Formatting prompt",
	"stt-timeout": "STT timeout",
};

function formatSettingName(setting: string): string {
	if (setting in SETTING_DISPLAY_NAMES) {
		return SETTING_DISPLAY_NAMES[setting as SettingKey];
	}
	return setting;
}

export default function App() {
	const [activeView, setActiveView] = useState<View>("home");
	const connectionState = useRecordingStore((s) => s.state);
	const hasShownConflictNotification = useRef(false);

	// Refresh server-side queries when connection is established
	useRefreshServerQueriesOnConnect(connectionState);

	// Fetch shortcut errors for startup notification
	const { data: shortcutErrors } = useShortcutErrors();

	// Show notification on startup if any hotkeys have conflicts
	useEffect(() => {
		if (hasShownConflictNotification.current) return;
		if (!shortcutErrors) return;

		const hasErrors =
			shortcutErrors.toggle_error ||
			shortcutErrors.hold_error ||
			shortcutErrors.paste_last_error;

		if (hasErrors) {
			hasShownConflictNotification.current = true;
			notifications.show({
				title: "Hotkey Conflict Detected",
				message:
					"Some hotkeys were disabled due to conflicts. Check settings to resolve.",
				color: "yellow",
				autoClose: 5000,
			});
		}
	}, [shortcutErrors]);

	// Listen for config response events from overlay window and show notifications
	// Store updates are now handled in ProvidersSettings (pessimistic updates)
	useEffect(() => {
		let isMounted = true;
		let unlisten: (() => void) | undefined;

		const handleConfigResponse = (response: ConfigResponse) => {
			match(response)
				.with({ type: "config-updated" }, ({ setting }) => {
					notifications.show({
						title: "Settings Updated",
						message: `${formatSettingName(setting)} updated successfully`,
						color: "green",
						autoClose: 2000,
					});
				})
				.with({ type: "config-error" }, ({ setting, error }) => {
					notifications.show({
						title: "Settings Error",
						message: `Failed to update ${formatSettingName(setting)}: ${error}`,
						color: "red",
						autoClose: 5000,
					});
					// No auto-fallback - dropdown reverts to previous value (pessimistic update)
				})
				.exhaustive();
		};

		tauriAPI.onConfigResponse(handleConfigResponse).then((fn) => {
			if (isMounted) {
				unlisten = fn;
			} else {
				// Component unmounted before listener was set up - clean up immediately
				fn();
			}
		});

		return () => {
			isMounted = false;
			unlisten?.();
		};
	}, []);

	// Listen for LLM errors from overlay and show detailed toast
	useEffect(() => {
		let isMounted = true;
		let unlisten: (() => void) | undefined;

		const handleLLMError = (error: LLMErrorPayload) => {
			notifications.show({
				title: error.fatal ? "Connection Error" : "Processing Error",
				message: error.message,
				color: "red",
				autoClose: 5000,
			});
		};

		tauriAPI.onLLMError(handleLLMError).then((fn) => {
			if (isMounted) {
				unlisten = fn;
			} else {
				fn();
			}
		});

		return () => {
			isMounted = false;
			unlisten?.();
		};
	}, []);

	return (
		<div className="app-layout">
			<Sidebar activeView={activeView} onViewChange={setActiveView} />
			{activeView === "home" ? <HomeView /> : <SettingsView />}
		</div>
	);
}
