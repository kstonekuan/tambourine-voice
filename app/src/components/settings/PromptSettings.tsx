import { Accordion, Loader } from "@mantine/core";
import { useEffect, useState } from "react";
import {
	useDefaultSections,
	useSettings,
	useUpdateCleanupPromptSections,
} from "../../lib/queries";
import { type CleanupPromptSections, tauriAPI } from "../../lib/tauri";
import { PromptSectionEditor } from "./PromptSectionEditor";

const DEFAULT_SECTIONS: CleanupPromptSections = {
	main: { enabled: true, content: null },
	advanced: { enabled: true, content: null },
	dictionary: { enabled: false, content: null },
};

type SectionKey = "main" | "advanced" | "dictionary";

interface LocalSectionState {
	enabled: boolean;
	content: string;
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
		main: { enabled: true, content: "" },
		advanced: { enabled: true, content: "" },
		dictionary: { enabled: false, content: "" },
	});

	// Track if each section has custom content (non-null, non-empty string)
	const mainContent = settings?.cleanup_prompt_sections?.main?.content;
	const advancedContent = settings?.cleanup_prompt_sections?.advanced?.content;
	const dictionaryContent =
		settings?.cleanup_prompt_sections?.dictionary?.content;

	const hasCustomContent = {
		main: mainContent != null && mainContent !== "",
		advanced: advancedContent != null && advancedContent !== "",
		dictionary: dictionaryContent != null && dictionaryContent !== "",
	};

	// Sync local state with settings when loaded
	useEffect(() => {
		if (settings !== undefined && defaultSections !== undefined) {
			const sections = settings.cleanup_prompt_sections ?? DEFAULT_SECTIONS;

			setLocalSections({
				main: {
					enabled: sections.main.enabled,
					content: sections.main.content ?? defaultSections.main,
				},
				advanced: {
					enabled: sections.advanced.enabled,
					content: sections.advanced.content ?? defaultSections.advanced,
				},
				dictionary: {
					enabled: sections.dictionary.enabled,
					content: sections.dictionary.content ?? defaultSections.dictionary,
				},
			});
		}
	}, [settings, defaultSections]);

	// Helper to build CleanupPromptSections from local state with optional overrides
	const buildSections = (overrides?: {
		key: SectionKey;
		enabled?: boolean;
		content?: string | null;
	}): CleanupPromptSections => {
		const getContent = (key: SectionKey): string | null => {
			const content =
				overrides?.key === key && overrides.content !== undefined
					? overrides.content
					: localSections[key].content;

			// Return null if content matches default (to use server default)
			if (content === defaultSections?.[key]) {
				return null;
			}
			return content || null;
		};

		const getEnabled = (key: SectionKey): boolean => {
			return overrides?.key === key && overrides.enabled !== undefined
				? overrides.enabled
				: localSections[key].enabled;
		};

		return {
			main: { enabled: getEnabled("main"), content: getContent("main") },
			advanced: {
				enabled: getEnabled("advanced"),
				content: getContent("advanced"),
			},
			dictionary: {
				enabled: getEnabled("dictionary"),
				content: getContent("dictionary"),
			},
		};
	};

	// Save all sections to Tauri and notify overlay window to sync to server
	const saveAllSections = (sections: CleanupPromptSections) => {
		updateCleanupPromptSections.mutate(sections, {
			onSuccess: () => {
				tauriAPI.emitSettingsChanged();
			},
		});
	};

	// Generic toggle handler
	const handleToggle = (key: SectionKey, checked: boolean) => {
		setLocalSections((prev) => ({
			...prev,
			[key]: { ...prev[key], enabled: checked },
		}));
		saveAllSections(buildSections({ key, enabled: checked }));
	};

	// Generic save handler
	const handleSave = (key: SectionKey, content: string) => {
		setLocalSections((prev) => ({
			...prev,
			[key]: { ...prev[key], content },
		}));
		saveAllSections(buildSections({ key, content }));
	};

	// Generic reset handler
	const handleReset = (key: SectionKey) => {
		const defaultContent = defaultSections?.[key] ?? "";
		setLocalSections((prev) => ({
			...prev,
			[key]: { ...prev[key], content: defaultContent },
		}));
		saveAllSections(buildSections({ key, content: null }));
	};

	return (
		<div className="settings-section animate-in animate-in-delay-4">
			<h3 className="settings-section-title">LLM Formatting Prompt</h3>
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
							onToggle={() => {}}
							onSave={(content) => handleSave("main", content)}
							onReset={() => handleReset("main")}
							isSaving={updateCleanupPromptSections.isPending}
						/>

						<PromptSectionEditor
							sectionKey="advanced-prompt"
							title="Advanced Features"
							description='Backtrack corrections ("scratch that") and list formatting'
							enabled={localSections.advanced.enabled}
							initialContent={localSections.advanced.content}
							defaultContent={defaultSections?.advanced ?? ""}
							hasCustom={hasCustomContent.advanced}
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
