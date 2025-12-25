import { Kbd, Loader, NavLink, Text, Title, Tooltip } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { Home, Settings } from "lucide-react";
import { useEffect, useState } from "react";
import { HistoryFeed } from "./components/HistoryFeed";
import { Logo } from "./components/Logo";
import {
	AudioSettings,
	HotkeySettings,
	PromptSettings,
	ProvidersSettings,
	ServerSettings,
} from "./components/settings";
import {
	DEFAULT_HOLD_HOTKEY,
	DEFAULT_PASTE_LAST_HOTKEY,
	DEFAULT_TOGGLE_HOTKEY,
} from "./lib/hotkeyDefaults";
import {
	useAvailableProvidersListener,
	useRefreshServerQueriesOnConnect,
	useSettings,
} from "./lib/queries";
import { type ConfigResponse, type HotkeyConfig, tauriAPI } from "./lib/tauri";
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

	const statusText = isConnecting
		? "Connecting..."
		: isConnected
			? "Connected"
			: "Disconnected";

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
				<p className="sidebar-footer-text">v0.1.0</p>
			</footer>
		</aside>
	);
}

function HotkeyDisplay({ config }: { config: HotkeyConfig }) {
	const parts = [
		...config.modifiers.map((m) => m.charAt(0).toUpperCase() + m.slice(1)),
		config.key,
	];

	return (
		<span className="kbd-combo">
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
					<HotkeyDisplay config={toggleHotkey} />
					<span className="instruction-desc">Press to start/stop</span>
				</div>
				<div className="instruction-method">
					<span className="instruction-label">Hold:</span>
					<HotkeyDisplay config={holdHotkey} />
					<span className="instruction-desc">Hold to record</span>
				</div>
				<div className="instruction-method">
					<span className="instruction-label">Paste:</span>
					<HotkeyDisplay config={pasteLastHotkey} />
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
	return (
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
			<ServerSettings />
		</div>
	);
}

function formatSettingName(setting: string): string {
	const names: Record<string, string> = {
		"stt-provider": "STT provider",
		"llm-provider": "LLM provider",
		"prompt-sections": "Formatting prompt",
		"stt-timeout": "STT timeout",
	};
	return names[setting] ?? setting;
}

export default function App() {
	const [activeView, setActiveView] = useState<View>("home");
	const connectionState = useRecordingStore((s) => s.state);

	// Listen for available providers from overlay window (must stay mounted)
	useAvailableProvidersListener();

	// Refresh server-side queries when connection is established
	useRefreshServerQueriesOnConnect(connectionState);

	// Listen for config response events from overlay window and show notifications
	useEffect(() => {
		let isMounted = true;
		let unlisten: (() => void) | undefined;

		const handleConfigResponse = (response: ConfigResponse) => {
			if (response.type === "config-updated") {
				notifications.show({
					title: "Settings Updated",
					message: `${formatSettingName(response.setting)} updated successfully`,
					color: "green",
					autoClose: 2000,
				});
			} else if (response.type === "config-error") {
				notifications.show({
					title: "Settings Error",
					message: `Failed to update ${formatSettingName(response.setting)}: ${response.error}`,
					color: "red",
					autoClose: 5000,
				});
			}
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

	return (
		<div className="app-layout">
			<Sidebar activeView={activeView} onViewChange={setActiveView} />
			{activeView === "home" ? <HomeView /> : <SettingsView />}
		</div>
	);
}
