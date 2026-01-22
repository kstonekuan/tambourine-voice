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

const DEFAULT_SECTIONS: CleanupPromptSections = {
	main: { enabled: true, mode: "auto" },
	advanced: { enabled: true, mode: "auto" },
	dictionary: { enabled: false, mode: "manual", content: "" },
};

type SectionKey = "main" | "advanced" | "dictionary";

interface LocalSectionState {
	enabled: boolean;
	content: string;
	auto: boolean;
}

interface LocalSections {
	main: LocalSectionState;
	advanced: LocalSectionState;
	dictionary: LocalSectionState;
}

export function PromptSettings() {
	const { data: settings } = useSettings();
	const { data: defaultSections, isLoading: isLoadingDefaultSections } =
		useDefaultSections();
	const updateCleanupPromptSections = useUpdateCleanupPromptSections();

	// Consolidated local state for all sections
	const [localSections, setLocalSections] = useState<LocalSections>({
		main: { enabled: true, content: "", auto: true },
		advanced: { enabled: true, content: "", auto: true },
		dictionary: { enabled: false, content: "", auto: false },
	});

	// Track if each section has custom content (manual mode with non-empty content)
	const getSectionContent = (
		section: PromptSection | undefined,
	): string | null => {
		if (!section) return null;
		return match(section)
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
		if (settings !== undefined && defaultSections !== undefined) {
			const sections = settings.cleanup_prompt_sections ?? DEFAULT_SECTIONS;

			// Helper to extract content from discriminated union
			const extractContent = (
				section: PromptSection,
				defaultContent: string,
			): string =>
				match(section)
					.with({ mode: "auto" }, () => defaultContent)
					.with({ mode: "manual" }, (s) => s.content || defaultContent)
					.exhaustive();

			setLocalSections({
				main: {
					enabled: sections.main.enabled,
					content: extractContent(sections.main, defaultSections.main),
					auto: sections.main.mode === "auto",
				},
				advanced: {
					enabled: sections.advanced.enabled,
					content: extractContent(sections.advanced, defaultSections.advanced),
					auto: sections.advanced.mode === "auto",
				},
				dictionary: {
					enabled: sections.dictionary.enabled,
					content: extractContent(
						sections.dictionary,
						defaultSections.dictionary,
					),
					auto: false, // Dictionary never has auto mode
				},
			});
		}
	}, [settings, defaultSections]);

	// Helper to build CleanupPromptSections from local state with optional overrides
	const buildSections = useCallback(
		(overrides?: {
			key: SectionKey;
			enabled?: boolean;
			content?: string | null;
			auto?: boolean;
		}): CleanupPromptSections => {
			const getEnabled = (key: SectionKey): boolean => {
				return overrides?.key === key && overrides.enabled !== undefined
					? overrides.enabled
					: localSections[key].enabled;
			};

			const getAuto = (key: SectionKey): boolean => {
				return overrides?.key === key && overrides.auto !== undefined
					? overrides.auto
					: localSections[key].auto;
			};

			const getContent = (key: SectionKey): string => {
				const content =
					overrides?.key === key && overrides.content !== undefined
						? overrides.content
						: localSections[key].content;
				return content || "";
			};

			// Build discriminated union based on mode
			const buildSection = (
				key: SectionKey,
			): CleanupPromptSections[SectionKey] => {
				const enabled = getEnabled(key);
				const isAuto = getAuto(key);

				if (isAuto) {
					return { enabled, mode: "auto" };
				}
				return { enabled, mode: "manual", content: getContent(key) };
			};

			return {
				main: buildSection("main"),
				advanced: buildSection("advanced"),
				dictionary: buildSection("dictionary"), // Dictionary is always manual
			};
		},
		[localSections],
	);

	// Save all sections to Tauri, which syncs to server
	const saveAllSections = useCallback(
		(sections: CleanupPromptSections) => {
			updateCleanupPromptSections.mutate(sections);
		},
		[updateCleanupPromptSections],
	);

	// Generic toggle handler
	const handleToggle = useCallback(
		(key: SectionKey, checked: boolean) => {
			setLocalSections((prev) => ({
				...prev,
				[key]: { ...prev[key], enabled: checked },
			}));
			saveAllSections(buildSections({ key, enabled: checked }));
		},
		[buildSections, saveAllSections],
	);

	// Generic save handler
	const handleSave = useCallback(
		(key: SectionKey, content: string) => {
			setLocalSections((prev) => ({
				...prev,
				[key]: { ...prev[key], content },
			}));
			saveAllSections(buildSections({ key, content }));
		},
		[buildSections, saveAllSections],
	);

	// Generic reset handler
	const handleReset = useCallback(
		(key: SectionKey) => {
			const defaultContent = defaultSections?.[key] ?? "";
			setLocalSections((prev) => ({
				...prev,
				[key]: { ...prev[key], content: defaultContent },
			}));
			saveAllSections(buildSections({ key, content: null }));
		},
		[defaultSections, buildSections, saveAllSections],
	);

	// Auto toggle handler - when switching to auto, content is sent as null (use server default)
	const handleAutoToggle = useCallback(
		(key: SectionKey, auto: boolean) => {
			setLocalSections((prev) => ({
				...prev,
				[key]: { ...prev[key], auto },
			}));
			saveAllSections(buildSections({ key, auto }));
		},
		[buildSections, saveAllSections],
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
							initialContent={localSections.main.content}
							defaultContent={defaultSections?.main ?? ""}
							hasCustom={hasCustomContent.main}
							auto={localSections.main.auto}
							onAutoToggle={(auto) => handleAutoToggle("main", auto)}
							showAutoToggle={true}
							onToggle={() => {}}
							onSave={(content) => handleSave("main", content)}
							onReset={() => handleReset("main")}
							isSaving={updateCleanupPromptSections.isPending}
						/>

						<PromptSectionEditor
							sectionKey="advanced-prompt"
							title="Advanced Features"
							description='E.g. backtrack corrections ("scratch that") and list formatting'
							enabled={localSections.advanced.enabled}
							initialContent={localSections.advanced.content}
							defaultContent={defaultSections?.advanced ?? ""}
							hasCustom={hasCustomContent.advanced}
							auto={localSections.advanced.auto}
							onAutoToggle={(auto) => handleAutoToggle("advanced", auto)}
							showAutoToggle={true}
							onToggle={(checked) => handleToggle("advanced", checked)}
							onSave={(content) => handleSave("advanced", content)}
							onReset={() => handleReset("advanced")}
							isSaving={updateCleanupPromptSections.isPending}
						/>

						<PromptSectionEditor
							sectionKey="dictionary-prompt"
							title="Personal Dictionary"
							description="Custom word mappings for technical terms"
							enabled={localSections.dictionary.enabled}
							initialContent={localSections.dictionary.content}
							defaultContent={defaultSections?.dictionary ?? ""}
							hasCustom={hasCustomContent.dictionary}
							showAutoToggle={false}
							onToggle={(checked) => handleToggle("dictionary", checked)}
							onSave={(content) => handleSave("dictionary", content)}
							onReset={() => handleReset("dictionary")}
							isSaving={updateCleanupPromptSections.isPending}
						/>
					</Accordion>
				)}
			</div>
		</div>
	);
}
