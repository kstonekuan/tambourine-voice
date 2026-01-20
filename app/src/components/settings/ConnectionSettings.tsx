import { Button, Loader, TextInput } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { Check, Copy, RefreshCw, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useSettings, useUpdateServerUrl } from "../../lib/queries";
import { DEFAULT_SERVER_URL, tauriAPI } from "../../lib/tauri";
import { useRecordingStore } from "../../stores/recordingStore";

type PingStatus = "idle" | "loading" | "success" | "error";

export function ConnectionSettings() {
	const { data: settings, isLoading } = useSettings();
	const updateServerUrl = useUpdateServerUrl();
	const [localUrl, setLocalUrl] = useState<string | null>(null);
	const [pingStatus, setPingStatus] = useState<PingStatus>("idle");
	const [clientUUID, setClientUUID] = useState<string | null>(null);
	const [uuidCopied, setUuidCopied] = useState(false);

	useEffect(() => {
		tauriAPI.getClientUUID().then(setClientUUID);
	}, []);

	const handleCopyUUID = useCallback(() => {
		if (!clientUUID) return;
		navigator.clipboard.writeText(clientUUID);
		setUuidCopied(true);
		setTimeout(() => setUuidCopied(false), 2000);
	}, [clientUUID]);

	const connectionState = useRecordingStore((s) => s.state);
	const setState = useRecordingStore((s) => s.setState);

	useEffect(() => {
		let unlisten: (() => void) | undefined;

		const setup = async () => {
			unlisten = await tauriAPI.onConnectionStateChanged((newState) => {
				setState(newState);
				// Reload UUID when connection becomes idle (may have re-registered)
				if (newState === "idle") {
					tauriAPI.getClientUUID().then(setClientUUID);
				}
			});
		};

		setup();

		return () => {
			unlisten?.();
		};
	}, [setState]);

	useEffect(() => {
		let unlisten: (() => void) | undefined;

		const setup = async () => {
			unlisten = await tauriAPI.onReconnectResult((result) => {
				if (!result.success) {
					notifications.show({
						title: "Reconnection Failed",
						message: result.error || "Could not reconnect to the server",
						color: "red",
						autoClose: 5000,
					});
				}
			});
		};

		setup();

		return () => {
			unlisten?.();
		};
	}, []);

	// Use local state if user is editing, otherwise use saved value
	const displayUrl = localUrl ?? settings?.server_url ?? DEFAULT_SERVER_URL;
	const hasChanges = localUrl !== null && localUrl !== settings?.server_url;

	const handleSave = () => {
		if (localUrl) {
			updateServerUrl.mutate(localUrl, {
				onSuccess: () => {
					setLocalUrl(null);
					// Reset ping status when URL changes
					setPingStatus("idle");
				},
			});
		}
	};

	const handleReset = () => {
		updateServerUrl.mutate(DEFAULT_SERVER_URL, {
			onSuccess: () => {
				setLocalUrl(null);
				setPingStatus("idle");
			},
		});
	};

	const handleKeyDown = (e: React.KeyboardEvent) => {
		if (e.key === "Enter" && hasChanges) {
			handleSave();
		}
	};

	const handleReconnect = useCallback(() => {
		tauriAPI.emitReconnect();
	}, []);

	const handlePing = useCallback(async () => {
		const urlToTest = displayUrl;
		setPingStatus("loading");

		try {
			const response = await fetch(`${urlToTest}/health`, {
				method: "GET",
				signal: AbortSignal.timeout(5000),
			});

			if (response.ok) {
				setPingStatus("success");
			} else {
				setPingStatus("error");
			}
		} catch {
			setPingStatus("error");
		}

		// Auto-clear status after 5 seconds
		setTimeout(() => {
			setPingStatus("idle");
		}, 5000);
	}, [displayUrl]);

	// Connection state display helpers
	const isConnecting = connectionState === "connecting";
	const isReconnecting = connectionState === "reconnecting";
	const isButtonDisabled = isConnecting || isReconnecting;

	const getStateDisplay = () => {
		switch (connectionState) {
			case "disconnected":
				return { text: "Disconnected", color: "var(--mantine-color-red-6)" };
			case "connecting":
				return {
					text: "Connecting...",
					color: "var(--mantine-color-yellow-6)",
				};
			case "reconnecting":
				return {
					text: "Reconnecting...",
					color: "var(--mantine-color-yellow-6)",
				};
			case "idle":
				return { text: "Connected", color: "var(--mantine-color-green-6)" };
			case "recording":
				return {
					text: "Connected (Recording)",
					color: "var(--mantine-color-green-6)",
				};
			case "processing":
				return {
					text: "Connected (Processing)",
					color: "var(--mantine-color-green-6)",
				};
			default:
				return { text: "Unknown", color: "var(--mantine-color-gray-6)" };
		}
	};

	const stateDisplay = getStateDisplay();

	return (
		<div className="settings-section animate-in animate-in-delay-4">
			<h3 className="settings-section-title">Connection</h3>

			{/* Status Row */}
			<div className="settings-card">
				<div
					className="settings-row"
					style={{ justifyContent: "space-between", alignItems: "center" }}
				>
					<div>
						<p className="settings-label">Status</p>
						<div style={{ display: "flex", alignItems: "center", gap: 8 }}>
							{isConnecting || isReconnecting ? (
								<Loader size={12} color="yellow" />
							) : (
								<span
									style={{
										width: 10,
										height: 10,
										borderRadius: "50%",
										backgroundColor: stateDisplay.color,
										display: "inline-block",
									}}
								/>
							)}
							<span
								style={{
									fontSize: "14px",
									color: stateDisplay.color,
									fontWeight: 500,
								}}
							>
								{stateDisplay.text}
							</span>
						</div>
					</div>
					<Button
						onClick={handleReconnect}
						disabled={isButtonDisabled}
						loading={isReconnecting}
						size="sm"
						variant="light"
						color="gray"
						leftSection={!isReconnecting && <RefreshCw size={14} />}
					>
						{isReconnecting ? "Reconnecting..." : "Reconnect"}
					</Button>
				</div>
			</div>

			{/* Server URL Row */}
			<div className="settings-card" style={{ marginTop: 12 }}>
				<div
					className="settings-row"
					style={{ flexDirection: "column", alignItems: "stretch", gap: 8 }}
				>
					<div>
						<p className="settings-label">Server URL</p>
						<p className="settings-description">
							The URL of the Tambourine server to connect to
						</p>
					</div>
					<div style={{ display: "flex", gap: 8, alignItems: "center" }}>
						<TextInput
							value={displayUrl}
							onChange={(e) => {
								setLocalUrl(e.currentTarget.value);
								setPingStatus("idle");
							}}
							onKeyDown={handleKeyDown}
							placeholder={DEFAULT_SERVER_URL}
							disabled={isLoading}
							style={{ flex: 1 }}
							styles={{
								input: {
									fontFamily: "monospace",
									fontSize: "13px",
								},
							}}
						/>
						<Button
							onClick={handlePing}
							loading={pingStatus === "loading"}
							size="sm"
							variant="light"
							color={
								pingStatus === "success"
									? "green"
									: pingStatus === "error"
										? "red"
										: "gray"
							}
							leftSection={
								pingStatus === "success" ? (
									<Check size={14} />
								) : pingStatus === "error" ? (
									<X size={14} />
								) : undefined
							}
						>
							{pingStatus === "success"
								? "Reachable"
								: pingStatus === "error"
									? "Unreachable"
									: "Test"}
						</Button>
						{hasChanges && (
							<Button
								onClick={handleSave}
								loading={updateServerUrl.isPending}
								size="sm"
								color="gray"
							>
								Save
							</Button>
						)}
						{settings?.server_url !== DEFAULT_SERVER_URL && !hasChanges && (
							<Button
								onClick={handleReset}
								loading={updateServerUrl.isPending}
								size="sm"
								variant="subtle"
								color="gray"
							>
								Reset
							</Button>
						)}
					</div>
				</div>
			</div>

			{/* Client ID Row */}
			<div className="settings-card" style={{ marginTop: 12 }}>
				<div
					className="settings-row"
					style={{ justifyContent: "space-between", alignItems: "center" }}
				>
					<div>
						<p className="settings-label">Client ID</p>
						<p
							style={{
								fontFamily: "monospace",
								fontSize: "13px",
								color: clientUUID
									? "var(--mantine-color-dimmed)"
									: "var(--mantine-color-gray-6)",
								fontStyle: clientUUID ? "normal" : "italic",
								margin: 0,
								marginTop: 4,
							}}
						>
							{clientUUID ?? "Not assigned yet"}
						</p>
					</div>
					{clientUUID && (
						<Button
							onClick={handleCopyUUID}
							size="sm"
							variant="light"
							color={uuidCopied ? "green" : "gray"}
							leftSection={
								uuidCopied ? <Check size={14} /> : <Copy size={14} />
							}
						>
							{uuidCopied ? "Copied" : "Copy"}
						</Button>
					)}
				</div>
			</div>
		</div>
	);
}
