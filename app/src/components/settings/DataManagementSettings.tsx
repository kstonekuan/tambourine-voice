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

			{/* Version info */}
			<Text
				size="xs"
				c="dimmed"
				ta="center"
				mt="xl"
				className="animate-in animate-in-delay-5"
			>
				v0.1.0
			</Text>

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
