import { Accordion, Button, Loader, Modal, Switch, Text } from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import { useCallback, useEffect, useState } from "react";
import { match } from "ts-pattern";
import {
	useDefaultSections,
	useSettings,
	useUpdateCleanupPromptSections,
	useUpdateLLMFormattingEnabled,
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

	// Modal for warning when disabling LLM formatting
	const [disableWarningOpened, disableWarningHandlers] = useDisclosure(false);

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
		</div>
	);
}
