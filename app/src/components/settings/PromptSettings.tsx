import { Accordion, Loader, Text } from "@mantine/core";
import { useCallback, useEffect, useState } from "react";
import { match } from "ts-pattern";
import {
	useDefaultSections,
	useSettings,
	useUpdateCleanupPromptSections,
} from "../../lib/queries";
import type { CleanupPromptSections, PromptSection } from "../../lib/tauri";
import { PromptSectionEditor } from "./PromptSectionEditor";
import type { MutationStatus } from "./StatusIndicator";

const DEFAULT_SECTIONS: CleanupPromptSections = {
	main: { mode: "auto" },
	advanced: { mode: "auto" },
	dictionary: { mode: "disabled" },
};

type SectionKey = "main" | "advanced" | "dictionary";

export function PromptSettings() {
	const { data: settings } = useSettings();
	const { data: defaultSections, isLoading: isLoadingDefaultSections } =
		useDefaultSections();
	const updateCleanupPromptSections = useUpdateCleanupPromptSections();

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

	// Track if each section has custom content (manual mode with non-empty content)
	const getSectionContent = (
		section: PromptSection | undefined,
	): string | null => {
		if (!section) return null;
		return match(section)
			.with({ mode: "disabled" }, () => null)
			.with({ mode: "auto" }, () => null)
			.with({ mode: "manual" }, (s) => s.content)
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

	// Generic toggle handler - toggles between disabled and auto/manual
	const handleToggle = useCallback(
		(key: SectionKey, checked: boolean) => {
			const newSection: PromptSection = checked
				? { mode: "auto" }
				: { mode: "disabled" };
			setLocalSections((prev) => ({
				...prev,
				[key]: newSection,
			}));
			saveAllSections(key, buildSections({ key, section: newSection }));
		},
		[buildSections, saveAllSections],
	);

	// Generic save handler - saves content in manual mode
	const handleSave = useCallback(
		(key: SectionKey, content: string) => {
			const newSection: PromptSection = { mode: "manual", content };
			setLocalSections((prev) => ({
				...prev,
				[key]: newSection,
			}));
			saveAllSections(key, buildSections({ key, section: newSection }));
		},
		[buildSections, saveAllSections],
	);

	// Generic reset handler - resets to auto mode
	const handleReset = useCallback(
		(key: SectionKey) => {
			const newSection: PromptSection = { mode: "auto" };
			setLocalSections((prev) => ({
				...prev,
				[key]: newSection,
			}));
			saveAllSections(key, buildSections({ key, section: newSection }));
		},
		[buildSections, saveAllSections],
	);

	// Auto toggle handler - switches between auto and manual mode
	const handleAutoToggle = useCallback(
		(key: SectionKey, auto: boolean) => {
			const currentSection = localSections[key];
			const newSection: PromptSection = auto
				? { mode: "auto" }
				: match(currentSection)
						.with({ mode: "manual" }, (s) => s) // Keep existing manual content
						.otherwise(() => ({
							mode: "manual",
							content: defaultSections?.[key] ?? "",
						}));
			setLocalSections((prev) => ({
				...prev,
				[key]: newSection,
			}));
			saveAllSections(key, buildSections({ key, section: newSection }));
		},
		[localSections, defaultSections, buildSections, saveAllSections],
	);

	return (
		<div className="settings-section animate-in animate-in-delay-4">
			<h3 className="settings-section-title">LLM Formatting Prompt</h3>
			<Text size="xs" c="dimmed" mb="sm">
				Custom prompts are stored locally. Consider backing up your
				customizations externally.
			</Text>
			<div className="settings-card">
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
							initialContent={match(localSections.main)
								.with({ mode: "disabled" }, () => "")
								.with({ mode: "auto" }, () => defaultSections?.main ?? "")
								.with({ mode: "manual" }, (s) => s.content)
								.exhaustive()}
							defaultContent={defaultSections?.main ?? ""}
							hasCustom={hasCustomContent.main}
							auto={match(localSections.main)
								.with({ mode: "disabled" }, () => false)
								.with({ mode: "auto" }, () => true)
								.with({ mode: "manual" }, () => false)
								.exhaustive()}
							onAutoToggle={(auto) => handleAutoToggle("main", auto)}
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
							enabled={match(localSections.advanced)
								.with({ mode: "disabled" }, () => false)
								.with({ mode: "auto" }, () => true)
								.with({ mode: "manual" }, () => true)
								.exhaustive()}
							initialContent={match(localSections.advanced)
								.with({ mode: "disabled" }, () => "")
								.with({ mode: "auto" }, () => defaultSections?.advanced ?? "")
								.with({ mode: "manual" }, (s) => s.content)
								.exhaustive()}
							defaultContent={defaultSections?.advanced ?? ""}
							hasCustom={hasCustomContent.advanced}
							auto={match(localSections.advanced)
								.with({ mode: "disabled" }, () => false)
								.with({ mode: "auto" }, () => true)
								.with({ mode: "manual" }, () => false)
								.exhaustive()}
							onAutoToggle={(auto) => handleAutoToggle("advanced", auto)}
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
							enabled={match(localSections.dictionary)
								.with({ mode: "disabled" }, () => false)
								.with({ mode: "auto" }, () => true)
								.with({ mode: "manual" }, () => true)
								.exhaustive()}
							initialContent={match(localSections.dictionary)
								.with({ mode: "disabled" }, () => "")
								.with({ mode: "auto" }, () => defaultSections?.dictionary ?? "")
								.with({ mode: "manual" }, (s) => s.content)
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
		</div>
	);
}
