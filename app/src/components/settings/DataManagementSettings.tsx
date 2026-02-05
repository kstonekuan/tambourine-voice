import {
	ActionIcon,
	Button,
	Group,
	Modal,
	Radio,
	Stack,
	Text,
	TextInput,
	Tooltip,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AlertTriangle, Download, RotateCcw, Upload } from "lucide-react";
import { useState } from "react";
import { match } from "ts-pattern";
import {
	type ParsedExportFile,
	useExportData,
	useFactoryReset,
	useImportData,
	useImportHistory,
	useImportPrompt,
	useImportSettings,
} from "../../lib/queries";
import type { HistoryImportStrategy } from "../../lib/tauri";

type ImportModalState =
	| { type: "closed" }
	| { type: "strategy"; historyFile: ParsedExportFile };

type ResetModalState =
	| { type: "closed" }
	| { type: "first_confirm" }
	| { type: "second_confirm" };

export function DataManagementSettings() {
	const exportData = useExportData();
	const importData = useImportData();
	const importSettings = useImportSettings();
	const importHistory = useImportHistory();
	const importPrompt = useImportPrompt();
	const factoryReset = useFactoryReset();

	const [importModalState, setImportModalState] = useState<ImportModalState>({
		type: "closed",
	});
	const [resetModalState, setResetModalState] = useState<ResetModalState>({
		type: "closed",
	});
	const [selectedStrategy, setSelectedStrategy] =
		useState<HistoryImportStrategy>("merge_deduplicate");
	const [resetConfirmText, setResetConfirmText] = useState("");

	const handleExport = () => {
		exportData.mutate();
	};

	const handleImport = async () => {
		const files = await importData.mutateAsync();

		if (files.length === 0) {
			// User cancelled
			return;
		}

		// Check for unknown files
		const unknownFiles = files.filter((f) => f.type === "unknown");
		if (unknownFiles.length > 0) {
			notifications.show({
				title: "Unknown File Format",
				message: `Could not recognize: ${unknownFiles.map((f) => f.filename).join(", ")}`,
				color: "yellow",
				autoClose: 5000,
			});
		}

		// Process settings files immediately
		const settingsFile = files.find((f) => f.type === "settings");
		if (settingsFile) {
			await importSettings.mutateAsync(settingsFile.content);
		}

		// Process prompt files immediately
		const promptFiles = files.filter((f) => f.type === "prompt");
		for (const promptFile of promptFiles) {
			if (promptFile.promptSection && promptFile.promptContent) {
				await importPrompt.mutateAsync({
					section: promptFile.promptSection,
					content: promptFile.promptContent,
				});
			}
		}

		// If there's a history file, show the strategy modal
		const historyFile = files.find((f) => f.type === "history");
		if (historyFile) {
			setImportModalState({ type: "strategy", historyFile });
		}
	};

	const handleHistoryImport = async () => {
		if (importModalState.type !== "strategy") return;

		await importHistory.mutateAsync({
			content: importModalState.historyFile.content,
			strategy: selectedStrategy,
		});

		setImportModalState({ type: "closed" });
	};

	const handleFactoryResetClick = () => {
		setResetModalState({ type: "first_confirm" });
	};

	const handleFirstConfirm = () => {
		setResetModalState({ type: "second_confirm" });
	};

	const handleFinalReset = async () => {
		await factoryReset.mutateAsync();
		setResetModalState({ type: "closed" });
		setResetConfirmText("");
	};

	const closeResetModal = () => {
		setResetModalState({ type: "closed" });
		setResetConfirmText("");
	};

	const isResetConfirmValid = resetConfirmText.toUpperCase() === "RESET";

	return (
		<>
			<div className="settings-section animate-in animate-in-delay-5">
				<h3 className="settings-section-title">Data Management</h3>

				{/* Export/Import Row */}
				<div className="settings-card">
					<div
						className="settings-row"
						style={{ justifyContent: "space-between", alignItems: "center" }}
					>
						<div>
							<p className="settings-label">Export & Import</p>
							<p className="settings-description">
								Export your settings, history, and custom prompts, or import
								from a previous export
							</p>
						</div>
						<Group gap="sm">
							<Tooltip label="Export Data" withArrow>
								<ActionIcon
									onClick={handleExport}
									loading={exportData.isPending}
									size="lg"
									variant="light"
									color="gray"
									aria-label="Export Data"
								>
									<Download size={16} />
								</ActionIcon>
							</Tooltip>
							<Tooltip label="Import Data" withArrow>
								<ActionIcon
									onClick={handleImport}
									loading={importData.isPending}
									size="lg"
									variant="light"
									color="gray"
									aria-label="Import Data"
								>
									<Upload size={16} />
								</ActionIcon>
							</Tooltip>
						</Group>
					</div>
				</div>

				{/* Factory Reset Row */}
				<div className="settings-card" style={{ marginTop: 12 }}>
					<div
						className="settings-row"
						style={{ justifyContent: "space-between", alignItems: "center" }}
					>
						<div>
							<p className="settings-label">Factory Reset</p>
							<p className="settings-description">
								Reset all settings to defaults and clear transcription history
							</p>
						</div>
						<Button
							onClick={handleFactoryResetClick}
							leftSection={<RotateCcw size={16} />}
							variant="light"
							color="red"
						>
							Factory Reset
						</Button>
					</div>
				</div>
			</div>

			{/* Version info and links */}
			<Stack
				align="center"
				gap={4}
				mt="xl"
				className="animate-in animate-in-delay-5"
			>
				<Group gap={4}>
					<Tooltip label="GitHub" withArrow>
						<ActionIcon
							size="xs"
							variant="subtle"
							color="gray"
							onClick={() =>
								openUrl("https://github.com/kstonekuan/tambourine-voice")
							}
						>
							<svg
								viewBox="0 0 24 24"
								fill="currentColor"
								width={14}
								height={14}
								role="img"
								aria-label="GitHub"
							>
								<title>GitHub</title>
								<path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12" />
							</svg>
						</ActionIcon>
					</Tooltip>
					<Tooltip label="Discord" withArrow>
						<ActionIcon
							size="xs"
							variant="subtle"
							color="gray"
							onClick={() => openUrl("https://discord.gg/dUyuXWVJ2a")}
						>
							<svg
								viewBox="0 0 24 24"
								fill="currentColor"
								width={14}
								height={14}
								role="img"
								aria-label="Discord"
							>
								<title>Discord</title>
								<path d="M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.37a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 0 0 .031.057 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028 14.09 14.09 0 0 0 1.226-1.994.076.076 0 0 0-.041-.106 13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128 10.2 10.2 0 0 0 .372-.292.074.074 0 0 1 .077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 0 1 .078.01c.12.098.246.198.373.292a.077.077 0 0 1-.006.127 12.299 12.299 0 0 1-1.873.892.077.077 0 0 0-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.03.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.03zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z" />
							</svg>
						</ActionIcon>
					</Tooltip>
				</Group>
				<Text size="xs" c="dimmed">
					v0.1.0
				</Text>
			</Stack>

			{/* History Import Strategy Modal */}
			<Modal
				opened={importModalState.type === "strategy"}
				onClose={() => setImportModalState({ type: "closed" })}
				title="Import History"
				centered
			>
				<Stack gap="md">
					<Text size="sm" c="dimmed">
						How would you like to handle existing history entries?
					</Text>

					<Radio.Group
						value={selectedStrategy}
						onChange={(value) =>
							setSelectedStrategy(value as HistoryImportStrategy)
						}
					>
						<Stack gap="sm">
							<Radio
								value="merge_deduplicate"
								label="Merge (skip duplicates)"
								description="Add new entries, skip ones that already exist"
							/>
							<Radio
								value="merge_append"
								label="Merge (keep all)"
								description="Add all imported entries alongside existing ones"
							/>
							<Radio
								value="replace"
								label="Replace"
								description="Delete all existing entries and use imported ones"
							/>
						</Stack>
					</Radio.Group>

					<Group justify="flex-end" mt="md">
						<Button
							variant="subtle"
							onClick={() => setImportModalState({ type: "closed" })}
						>
							Cancel
						</Button>
						<Button
							onClick={handleHistoryImport}
							loading={importHistory.isPending}
						>
							Import
						</Button>
					</Group>
				</Stack>
			</Modal>

			{/* Factory Reset Confirmation Modals */}
			<Modal
				opened={resetModalState.type !== "closed"}
				onClose={closeResetModal}
				title={
					<Group gap="xs">
						<AlertTriangle size={20} color="var(--mantine-color-red-6)" />
						<span>Factory Reset</span>
					</Group>
				}
				centered
			>
				{match(resetModalState)
					.with({ type: "first_confirm" }, () => (
						<Stack gap="md">
							<Text size="sm">
								Are you sure you want to reset all settings and clear your
								transcription history?
							</Text>
							<Text size="sm" c="red" fw={500}>
								This action cannot be undone.
							</Text>
							<Group justify="flex-end" mt="md">
								<Button variant="subtle" onClick={closeResetModal}>
									Cancel
								</Button>
								<Button color="red" onClick={handleFirstConfirm}>
									Continue
								</Button>
							</Group>
						</Stack>
					))
					.with({ type: "second_confirm" }, () => (
						<Stack gap="md">
							<Text size="sm" fw={500}>
								This will permanently delete:
							</Text>
							<ul style={{ margin: 0, paddingLeft: 20 }}>
								<li>
									<Text size="sm">All your custom settings</Text>
								</li>
								<li>
									<Text size="sm">All hotkey configurations</Text>
								</li>
								<li>
									<Text size="sm">All transcription history</Text>
								</li>
							</ul>
							<Text size="sm" c="dimmed" mt="xs">
								Type <strong>RESET</strong> below to confirm:
							</Text>
							<TextInput
								value={resetConfirmText}
								onChange={(e) => setResetConfirmText(e.currentTarget.value)}
								placeholder="Type RESET to confirm"
								styles={{
									input: {
										fontFamily: "monospace",
									},
								}}
							/>
							<Group justify="flex-end" mt="md">
								<Button variant="subtle" onClick={closeResetModal}>
									Cancel
								</Button>
								<Button
									color="red"
									onClick={handleFinalReset}
									disabled={!isResetConfirmValid}
									loading={factoryReset.isPending}
								>
									Reset Everything
								</Button>
							</Group>
						</Stack>
					))
					.with({ type: "closed" }, () => null)
					.exhaustive()}
			</Modal>
		</>
	);
}
