import {
	Accordion,
	Button,
	Loader,
	Modal,
	Switch,
	Text,
	Textarea,
} from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import { useCallback, useEffect, useState } from "react";
import { match } from "ts-pattern";
import {
	useDefaultSections,
	useMemoryMarkdownView,
	useSettings,
	useUpdateCleanupPromptSections,
	useUpdateLLMFormattingEnabled,
	useUpdateLLMTimeoutRawFallbackEnabled,
	useUpdateMemoryEnabled,
	useUpdateSendActiveAppContextEnabled,
} from "../../lib/queries";
import type { CleanupPromptSections, PromptSection } from "../../lib/tauri";
import { PromptSectionEditor } from "./PromptSectionEditor";
import { type MutationStatus, StatusIndicator } from "./StatusIndicator";

const DEFAULT_SECTIONS: CleanupPromptSections = {
	main: { enabled: true, mode: { mode: "auto" } },
	advanced: { enabled: true, mode: { mode: "auto" } },
	dictionary: { enabled: true, mode: { mode: "auto" } },
};

type SectionKey = "main" | "advanced" | "dictionary";

export function PromptSettings() {
	const { data: settings } = useSettings();
	const { data: defaultSections, isLoading: isLoadingDefaultSections } =
		useDefaultSections();
	const updateCleanupPromptSections = useUpdateCleanupPromptSections();
	const llmFormattingMutation = useUpdateLLMFormattingEnabled();
	const llmTimeoutRawFallbackMutation = useUpdateLLMTimeoutRawFallbackEnabled();
	const activeAppContextMutation = useUpdateSendActiveAppContextEnabled();
	const memoryEnabledMutation = useUpdateMemoryEnabled();
	const isMemoryEnabled = settings?.memory_enabled ?? false;
	const {
		data: memoryMarkdown,
		error: memoryMarkdownError,
		isError: hasMemoryMarkdownLoadError,
		isFetching: isMemoryMarkdownFetching,
		isLoading: isMemoryMarkdownLoading,
		refetch: refetchMemoryMarkdown,
	} = useMemoryMarkdownView(isMemoryEnabled);

	// Modal for warning when disabling LLM formatting
	const [disableWarningOpened, disableWarningHandlers] = useDisclosure(false);
	const [focusWarningOpened, focusWarningHandlers] = useDisclosure(false);
	const [memoryWarningOpened, memoryWarningHandlers] = useDisclosure(false);

	// Consolidated local state for all sections using discriminated union
	const [localSections, setLocalSections] =
		useState<CleanupPromptSections>(DEFAULT_SECTIONS);

	// Track which section is currently saving to show per-section status
	const [savingSectionKey, setSavingSectionKey] = useState<SectionKey | null>(
		null,
	);

	// Compute per-section mutation status
	const getSectionMutationStatus = (key: SectionKey): MutationStatus => {
		if (savingSectionKey !== key) return "idle";
		return updateCleanupPromptSections.status;
	};

	const getSectionContent = (
		section: PromptSection | undefined,
	): string | null => {
		if (!section) return null;
		return match(section.mode)
			.with({ mode: "auto" }, () => null)
			.with({ mode: "manual" }, (m) => m.content)
			.exhaustive();
	};

	const mainContent = getSectionContent(
		settings?.cleanup_prompt_sections?.main,
	);
	const advancedContent = getSectionContent(
		settings?.cleanup_prompt_sections?.advanced,
	);
	const dictionaryContent = getSectionContent(
		settings?.cleanup_prompt_sections?.dictionary,
	);

	const hasCustomContent = {
		main: mainContent != null && mainContent !== "",
		advanced: advancedContent != null && advancedContent !== "",
		dictionary: dictionaryContent != null && dictionaryContent !== "",
	};

	// Sync local state with settings when loaded
	useEffect(() => {
		if (settings !== undefined) {
			const sections = settings.cleanup_prompt_sections ?? DEFAULT_SECTIONS;
			setLocalSections(sections);
		}
	}, [settings]);

	// Helper to build CleanupPromptSections from local state with optional overrides
	const buildSections = useCallback(
		(overrides?: {
			key: SectionKey;
			section: PromptSection;
		}): CleanupPromptSections => {
			return {
				main:
					overrides?.key === "main" ? overrides.section : localSections.main,
				advanced:
					overrides?.key === "advanced"
						? overrides.section
						: localSections.advanced,
				dictionary:
					overrides?.key === "dictionary"
						? overrides.section
						: localSections.dictionary,
			};
		},
		[localSections],
	);

	// Save all sections to Tauri, which syncs to server
	const saveAllSections = useCallback(
		(key: SectionKey, sections: CleanupPromptSections) => {
			setSavingSectionKey(key);
			updateCleanupPromptSections.mutate(sections);
		},
		[updateCleanupPromptSections],
	);

	const handleToggle = useCallback(
		(key: SectionKey, checked: boolean) => {
			const currentSection = localSections[key];
			const newSection: PromptSection = {
				enabled: checked,
				mode: currentSection.mode,
			};
			setLocalSections((prev) => ({
				...prev,
				[key]: newSection,
			}));
			saveAllSections(key, buildSections({ key, section: newSection }));
		},
		[localSections, buildSections, saveAllSections],
	);

	const handleSave = useCallback(
		(key: SectionKey, content: string) => {
			const currentSection = localSections[key];
			const newSection: PromptSection = {
				enabled: currentSection.enabled,
				mode: { mode: "manual", content },
			};
			setLocalSections((prev) => ({
				...prev,
				[key]: newSection,
			}));
			saveAllSections(key, buildSections({ key, section: newSection }));
		},
		[localSections, buildSections, saveAllSections],
	);

	const handleReset = useCallback(
		(key: SectionKey) => {
			const currentSection = localSections[key];
			const newSection: PromptSection = {
				enabled: currentSection.enabled,
				mode: { mode: "manual", content: defaultSections?.[key] ?? "" },
			};
			setLocalSections((prev) => ({
				...prev,
				[key]: newSection,
			}));
			saveAllSections(key, buildSections({ key, section: newSection }));
		},
		[localSections, defaultSections, buildSections, saveAllSections],
	);

	const handleAutoToggle = useCallback(
		(key: SectionKey) => {
			const currentSection = localSections[key];

			const newMode = match(currentSection.mode)
				.with({ mode: "auto" }, () => ({
					mode: "manual" as const,
					content: defaultSections?.[key] ?? "",
				}))
				.with({ mode: "manual" }, () => ({ mode: "auto" as const }))
				.exhaustive();

			const newSection: PromptSection = {
				enabled: currentSection.enabled,
				mode: newMode,
			};

			setLocalSections((prev) => ({
				...prev,
				[key]: newSection,
			}));
			saveAllSections(key, buildSections({ key, section: newSection }));
		},
		[localSections, defaultSections, buildSections, saveAllSections],
	);

	// Check if LLM formatting is disabled
	const isLLMFormattingDisabled = settings?.llm_formatting_enabled === false;
	const memoryMarkdownErrorMessage =
		memoryMarkdownError instanceof Error
			? memoryMarkdownError.message
			: "Unable to load memory markdown.";

	const handleLLMFormattingToggle = (checked: boolean) => {
		if (!checked) {
			// Show warning when trying to disable
			disableWarningHandlers.open();
		} else {
			// Enable directly without warning
			llmFormattingMutation.mutate(true);
		}
	};

	const confirmDisableLLMFormatting = () => {
		llmFormattingMutation.mutate(false);
		disableWarningHandlers.close();
	};

	const handleActiveAppContextToggle = (checked: boolean) => {
		if (checked) {
			focusWarningHandlers.open();
		} else {
			activeAppContextMutation.mutate(false);
		}
	};

	const confirmEnableActiveAppContext = () => {
		activeAppContextMutation.mutate(true);
		focusWarningHandlers.close();
	};

	const handleMemoryToggle = (checked: boolean) => {
		if (checked) {
			memoryWarningHandlers.open();
		} else {
			memoryEnabledMutation.mutate(false);
		}
	};

	const confirmEnableMemory = () => {
		memoryEnabledMutation.mutate(true);
		memoryWarningHandlers.close();
	};

	return (
		<div className="settings-section animate-in animate-in-delay-4">
			<h3 className="settings-section-title">LLM Formatting</h3>
			<div className="settings-card" style={{ marginBottom: 16 }}>
				<div className="settings-row">
					<div>
						<div style={{ display: "flex", alignItems: "center", gap: 8 }}>
							<p className="settings-label">Enable LLM Formatting</p>
							<StatusIndicator status={llmFormattingMutation.status} />
						</div>
						<p className="settings-description">
							Format transcriptions using AI
						</p>
					</div>
					<Switch
						checked={settings?.llm_formatting_enabled ?? true}
						onChange={(event) =>
							handleLLMFormattingToggle(event.currentTarget.checked)
						}
						disabled={llmFormattingMutation.isPending}
						size="md"
						color="gray"
					/>
				</div>
				<div className="settings-row">
					<div>
						<div style={{ display: "flex", alignItems: "center", gap: 8 }}>
							<p className="settings-label">
								Fallback to Raw STT on LLM Timeout
							</p>
							<StatusIndicator status={llmTimeoutRawFallbackMutation.status} />
						</div>
						<p className="settings-description">
							If formatting times out, insert raw transcription instead of
							dropping text
						</p>
					</div>
					<Switch
						checked={settings?.llm_timeout_raw_fallback_enabled ?? false}
						onChange={(event) =>
							llmTimeoutRawFallbackMutation.mutate(event.currentTarget.checked)
						}
						disabled={llmTimeoutRawFallbackMutation.isPending}
						size="md"
						color="gray"
					/>
				</div>
				<div className="settings-row">
					<div>
						<div style={{ display: "flex", alignItems: "center", gap: 8 }}>
							<p className="settings-label">
								Use Active App Context for Formatting
							</p>
							<StatusIndicator status={activeAppContextMutation.status} />
						</div>
						<p className="settings-description">
							Share active app/window context with the server to improve
							formatting quality
						</p>
					</div>
					<Switch
						checked={settings?.send_active_app_context_enabled ?? false}
						onChange={(event) =>
							handleActiveAppContextToggle(event.currentTarget.checked)
						}
						disabled={activeAppContextMutation.isPending}
						size="md"
						color="gray"
					/>
				</div>
				<div style={{ paddingTop: 12 }}>
					<Accordion variant="separated" radius="md">
						<Accordion.Item value="memory-markdown">
							<Accordion.Control>
								<div
									style={{
										display: "flex",
										justifyContent: "space-between",
										alignItems: "center",
										width: "100%",
										paddingRight: 12,
									}}
								>
									<div>
										<div
											style={{ display: "flex", alignItems: "center", gap: 8 }}
										>
											<p className="settings-label">Enable Memory</p>
											<StatusIndicator status={memoryEnabledMutation.status} />
										</div>
										<p className="settings-description">
											Store cross-session context in a local markdown file
										</p>
									</div>
									<Switch
										checked={isMemoryEnabled}
										onChange={(event) => {
											event.stopPropagation();
											handleMemoryToggle(event.currentTarget.checked);
										}}
										onClick={(event) => event.stopPropagation()}
										disabled={memoryEnabledMutation.isPending}
										size="md"
										color="gray"
									/>
								</div>
							</Accordion.Control>
							<Accordion.Panel>
								{!isMemoryEnabled ? (
									<Text size="sm" c="dimmed">
										Memory content is hidden while memory is disabled.
									</Text>
								) : isMemoryMarkdownLoading ? (
									<div
										style={{
											display: "flex",
											justifyContent: "center",
											padding: "12px 0",
											width: "100%",
										}}
									>
										<Loader size="sm" color="gray" />
									</div>
								) : hasMemoryMarkdownLoadError ? (
									<div
										style={{
											width: "100%",
											display: "flex",
											flexDirection: "column",
											gap: 8,
										}}
									>
										<Text size="sm" c="red">
											Failed to load memory content:{" "}
											{memoryMarkdownErrorMessage}
										</Text>
										<div>
											<Button
												variant="light"
												size="xs"
												onClick={() => {
													void refetchMemoryMarkdown();
												}}
												loading={isMemoryMarkdownFetching}
											>
												Retry
											</Button>
										</div>
									</div>
								) : (
									<Textarea
										value={memoryMarkdown ?? ""}
										readOnly
										autosize
										minRows={6}
										maxRows={14}
										styles={{
											input: {
												backgroundColor: "var(--bg-elevated)",
												borderColor: "var(--border-default)",
												color: "var(--text-primary)",
												fontFamily: "monospace",
												fontSize: "13px",
											},
										}}
									/>
								)}
							</Accordion.Panel>
						</Accordion.Item>
					</Accordion>
				</div>
			</div>

			{isLLMFormattingDisabled ? (
				<Text size="xs" c="yellow" mb="sm">
					LLM formatting is disabled. The prompts below are not used.
				</Text>
			) : (
				<Text size="xs" c="dimmed" mb="sm">
					Custom prompts are stored locally, consider backing up externally. You
					can export all settings including prompts below.
				</Text>
			)}
			<div
				className="settings-card"
				style={{
					opacity: isLLMFormattingDisabled ? 0.5 : 1,
					pointerEvents: isLLMFormattingDisabled ? "none" : "auto",
				}}
			>
				{isLoadingDefaultSections ? (
					<div
						style={{
							display: "flex",
							justifyContent: "center",
							padding: "20px",
						}}
					>
						<Loader size="sm" color="gray" />
					</div>
				) : (
					<Accordion variant="separated" radius="md">
						<PromptSectionEditor
							sectionKey="main-prompt"
							title="Core Formatting Rules"
							description="Filler word removal, punctuation, capitalization"
							enabled={true}
							hideToggle={true}
							initialContent={match(localSections.main.mode)
								.with({ mode: "auto" }, () => defaultSections?.main ?? "")
								.with({ mode: "manual" }, (m) => m.content)
								.exhaustive()}
							defaultContent={defaultSections?.main ?? ""}
							hasCustom={hasCustomContent.main}
							auto={match(localSections.main.mode)
								.with({ mode: "auto" }, () => true)
								.with({ mode: "manual" }, () => false)
								.exhaustive()}
							onAutoToggle={() => handleAutoToggle("main")}
							showAutoToggle={true}
							onToggle={() => {}}
							onSave={(content) => handleSave("main", content)}
							onReset={() => handleReset("main")}
							isSaving={updateCleanupPromptSections.isPending}
							mutationStatus={getSectionMutationStatus("main")}
						/>

						<PromptSectionEditor
							sectionKey="advanced-prompt"
							title="Advanced Features"
							description='E.g. backtrack corrections ("scratch that") and list formatting'
							enabled={localSections.advanced.enabled}
							initialContent={match(localSections.advanced.mode)
								.with({ mode: "auto" }, () => defaultSections?.advanced ?? "")
								.with({ mode: "manual" }, (m) => m.content)
								.exhaustive()}
							defaultContent={defaultSections?.advanced ?? ""}
							hasCustom={hasCustomContent.advanced}
							auto={match(localSections.advanced.mode)
								.with({ mode: "auto" }, () => true)
								.with({ mode: "manual" }, () => false)
								.exhaustive()}
							onAutoToggle={() => handleAutoToggle("advanced")}
							showAutoToggle={true}
							onToggle={(checked) => handleToggle("advanced", checked)}
							onSave={(content) => handleSave("advanced", content)}
							onReset={() => handleReset("advanced")}
							isSaving={updateCleanupPromptSections.isPending}
							mutationStatus={getSectionMutationStatus("advanced")}
						/>

						<PromptSectionEditor
							sectionKey="dictionary-prompt"
							title="Personal Dictionary"
							description="Custom word mappings for technical terms"
							enabled={localSections.dictionary.enabled}
							initialContent={match(localSections.dictionary.mode)
								.with({ mode: "auto" }, () => defaultSections?.dictionary ?? "")
								.with({ mode: "manual" }, (m) => m.content)
								.exhaustive()}
							defaultContent={defaultSections?.dictionary ?? ""}
							hasCustom={hasCustomContent.dictionary}
							showAutoToggle={false}
							onToggle={(checked) => handleToggle("dictionary", checked)}
							onSave={(content) => handleSave("dictionary", content)}
							onReset={() => handleReset("dictionary")}
							isSaving={updateCleanupPromptSections.isPending}
							mutationStatus={getSectionMutationStatus("dictionary")}
						/>
					</Accordion>
				)}
			</div>

			{/* Warning modal when disabling LLM formatting */}
			<Modal
				opened={disableWarningOpened}
				onClose={disableWarningHandlers.close}
				title="Are you sure?"
				centered
				size="md"
			>
				<Text size="sm" mb="md">
					Disabling LLM formatting might negatively impact your experience. Only
					disable this if you know what you're doing or need raw STT output.
				</Text>
				<div style={{ display: "flex", gap: 12, justifyContent: "flex-end" }}>
					<Button variant="default" onClick={disableWarningHandlers.close}>
						Cancel
					</Button>
					<Button color="red" onClick={confirmDisableLLMFormatting}>
						Disable Formatting
					</Button>
				</div>
			</Modal>

			{/* Warning modal when enabling active app context */}
			<Modal
				opened={focusWarningOpened}
				onClose={focusWarningHandlers.close}
				title="Enable active app context?"
				centered
				size="md"
			>
				<Text size="sm" mb="md">
					This experimental feature can improve dictation quality by adapting
					formatting to your active app or window. It might send personal data
					about your active app/window to the server, and if you connect to a
					cloud service, that data will be sent to the cloud service as well.
				</Text>
				<div style={{ display: "flex", gap: 12, justifyContent: "flex-end" }}>
					<Button variant="default" onClick={focusWarningHandlers.close}>
						Cancel
					</Button>
					<Button color="orange" onClick={confirmEnableActiveAppContext}>
						Enable Active App Context
					</Button>
				</div>
			</Modal>
			<Modal
				opened={memoryWarningOpened}
				onClose={memoryWarningHandlers.close}
				title="Enable memory?"
				centered
				size="md"
			>
				<Text size="sm" mb="md">
					This experimental feature can improve dictation quality by adapting
					formatting to your active app or window. It might send personal data
					about your active app/window to the server, and if you connect to a
					cloud service, that data will be sent to the cloud service as well.
				</Text>
				<div style={{ display: "flex", gap: 12, justifyContent: "flex-end" }}>
					<Button variant="default" onClick={memoryWarningHandlers.close}>
						Cancel
					</Button>
					<Button color="orange" onClick={confirmEnableMemory}>
						Enable Memory
					</Button>
				</div>
			</Modal>
		</div>
	);
}
