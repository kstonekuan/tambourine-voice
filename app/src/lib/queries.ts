import { notifications } from "@mantine/notifications";
import type { QueryClient } from "@tanstack/react-query";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";
import { match } from "ts-pattern";

// =============================================================================
// Query Stale Time Constants
// =============================================================================

/** Data that never goes stale (static configuration, settings) */
const STALE_TIME_INFINITE = Number.POSITIVE_INFINITY;

/** Provider lists refresh after 30 seconds */
const STALE_TIME_PROVIDERS_MS = 30_000;

/** Always refetch to get the latest data (e.g., errors) */
const STALE_TIME_ALWAYS_REFETCH = 0;

// =============================================================================
// Notification Helpers
// =============================================================================

function showSettingsSuccess(message: string): void {
	notifications.show({
		title: "Settings Updated",
		message,
		color: "green",
		autoClose: 2000,
	});
}

function showSettingsError(message: string): void {
	notifications.show({
		title: "Settings Error",
		message,
		color: "red",
		autoClose: 5000,
	});
}

import {
	type AppSettings,
	type AvailableProvidersData,
	type CleanupPromptSections,
	type ConnectionState,
	configAPI,
	type DetectedFileType,
	getProviderIdFromSelection,
	type HistoryImportStrategy,
	type HotkeyConfig,
	type LLMProviderSelection,
	type PromptSectionName,
	parseLLMProviderSelection,
	parseSTTProviderSelection,
	type STTProviderSelection,
	tauriAPI,
	validateHotkeyNotDuplicate,
} from "./tauri";

/**
 * Hook to refresh all server-side queries when connection is established.
 * Call this from a component that has access to the connection state.
 */
export function useRefreshServerQueriesOnConnect(
	connectionState: ConnectionState,
) {
	const queryClient = useQueryClient();
	const previousStateRef = useRef(connectionState);

	useEffect(() => {
		const wasPreviouslyDisconnected =
			previousStateRef.current === "disconnected" ||
			previousStateRef.current === "connecting" ||
			previousStateRef.current === "reconnecting";
		const isCurrentlyConnected =
			connectionState === "idle" ||
			connectionState === "recording" ||
			connectionState === "processing";

		if (wasPreviouslyDisconnected && isCurrentlyConnected) {
			// Invalidate server-side queries (static data that may have changed)
			queryClient.invalidateQueries({ queryKey: ["availableProviders"] });
			queryClient.invalidateQueries({ queryKey: ["defaultSections"] });
		}

		previousStateRef.current = connectionState;
	}, [connectionState, queryClient]);
}

export function useServerUrl() {
	return useQuery({
		queryKey: ["serverUrl"],
		queryFn: () => invoke<string>("get_server_url"),
		staleTime: STALE_TIME_INFINITE,
	});
}

export function useTypeText() {
	return useMutation({
		mutationFn: (text: string) => invoke("type_text", { text }),
	});
}

// Settings queries and mutations
export function useSettings() {
	return useQuery({
		queryKey: ["settings"],
		queryFn: () => tauriAPI.getSettings(),
		staleTime: STALE_TIME_INFINITE,
	});
}

type HotkeyType = "toggle" | "hold" | "paste_last";

// Shared internal function for hotkey mutations
async function executeHotkeyUpdate(
	hotkeyType: HotkeyType,
	updateFn: (hotkey: HotkeyConfig) => Promise<void>,
	hotkey: HotkeyConfig,
): Promise<void> {
	const settings = await tauriAPI.getSettings();
	const error = validateHotkeyNotDuplicate(
		hotkey,
		{
			toggle: settings.toggle_hotkey,
			hold: settings.hold_hotkey,
			paste_last: settings.paste_last_hotkey,
		},
		hotkeyType,
	);
	if (error) throw new Error(error);

	await updateFn(hotkey);
	await tauriAPI.registerShortcuts();
}

export function useUpdateToggleHotkey() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (hotkey: HotkeyConfig) =>
			executeHotkeyUpdate("toggle", tauriAPI.updateToggleHotkey, hotkey),
		onSuccess: () => {
			showSettingsSuccess("Toggle hotkey updated successfully");
		},
		onError: (error) => {
			showSettingsError(`Failed to update toggle hotkey: ${error.message}`);
		},
		onSettled: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			queryClient.refetchQueries({ queryKey: ["shortcutErrors"] });
		},
	});
}

export function useUpdateHoldHotkey() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (hotkey: HotkeyConfig) =>
			executeHotkeyUpdate("hold", tauriAPI.updateHoldHotkey, hotkey),
		onSuccess: () => {
			showSettingsSuccess("Hold hotkey updated successfully");
		},
		onError: (error) => {
			showSettingsError(`Failed to update hold hotkey: ${error.message}`);
		},
		onSettled: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			queryClient.refetchQueries({ queryKey: ["shortcutErrors"] });
		},
	});
}

export function useUpdatePasteLastHotkey() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (hotkey: HotkeyConfig) =>
			executeHotkeyUpdate("paste_last", tauriAPI.updatePasteLastHotkey, hotkey),
		onSuccess: () => {
			showSettingsSuccess("Paste last hotkey updated successfully");
		},
		onError: (error) => {
			showSettingsError(`Failed to update paste last hotkey: ${error.message}`);
		},
		onSettled: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			queryClient.refetchQueries({ queryKey: ["shortcutErrors"] });
		},
	});
}

export function useUpdateSelectedMic() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (micId: string | null) => tauriAPI.updateSelectedMic(micId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			tauriAPI.emitSettingsChanged();
			showSettingsSuccess("Microphone selection updated successfully");
		},
		onError: (error) => {
			showSettingsError(`Failed to update microphone: ${error.message}`);
		},
	});
}

export function useUpdateSoundEnabled() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (enabled: boolean) => tauriAPI.updateSoundEnabled(enabled),
		onSuccess: (_data, enabled) => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			showSettingsSuccess(`Sound feedback ${enabled ? "enabled" : "disabled"}`);
		},
		onError: (error) => {
			showSettingsError(`Failed to update sound setting: ${error.message}`);
		},
	});
}

export function useUpdateAutoMuteAudio() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (enabled: boolean) => tauriAPI.updateAutoMuteAudio(enabled),
		onSuccess: (_data, enabled) => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			showSettingsSuccess(`Auto-mute ${enabled ? "enabled" : "disabled"}`);
		},
		onError: (error) => {
			showSettingsError(`Failed to update auto-mute setting: ${error.message}`);
		},
	});
}

export function useIsAudioMuteSupported() {
	return useQuery({
		queryKey: ["audioMuteSupported"],
		queryFn: () => tauriAPI.isAudioMuteSupported(),
		staleTime: STALE_TIME_INFINITE,
	});
}

export function useUpdateCleanupPromptSections() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (sections: CleanupPromptSections | null) =>
			tauriAPI.updateCleanupPromptSections(sections),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			showSettingsSuccess("Formatting prompt updated successfully");
		},
		onError: (error) => {
			showSettingsError(`Failed to update formatting prompt: ${error.message}`);
		},
	});
}

export function useResetHotkeysToDefaults() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: async () => {
			await tauriAPI.resetHotkeysToDefaults();
			await tauriAPI.registerShortcuts();
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			queryClient.invalidateQueries({ queryKey: ["shortcutErrors"] });
			showSettingsSuccess("Hotkeys reset to defaults");
		},
		onError: (error) => {
			console.error("Reset hotkeys failed:", error);
			showSettingsError(`Failed to reset hotkeys: ${error.message}`);
		},
	});
}

export function useShortcutErrors() {
	return useQuery({
		queryKey: ["shortcutErrors"],
		queryFn: () => tauriAPI.getShortcutErrors(),
		staleTime: STALE_TIME_ALWAYS_REFETCH,
	});
}

const HOTKEY_TYPE_LABELS = {
	toggle: "Toggle",
	hold: "Hold",
	paste_last: "Paste last",
} as const;

export function useSetHotkeyEnabled() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: async ({
			hotkeyType,
			enabled,
		}: {
			hotkeyType: "toggle" | "hold" | "paste_last";
			enabled: boolean;
		}) => {
			await tauriAPI.setHotkeyEnabled(hotkeyType, enabled);
			const result = await tauriAPI.registerShortcuts();

			// Check for errors regardless of enabled state
			const errorKey = `${hotkeyType}_error` as keyof typeof result.errors;
			if (result.errors[errorKey]) {
				throw new Error(result.errors[errorKey] as string);
			}

			return { result, hotkeyType, enabled };
		},
		onSuccess: ({ hotkeyType, enabled }) => {
			const label = HOTKEY_TYPE_LABELS[hotkeyType];
			showSettingsSuccess(
				`${label} hotkey ${enabled ? "enabled" : "disabled"}`,
			);
		},
		onError: (error) => {
			showSettingsError(`Failed to update hotkey: ${error.message}`);
		},
		onSettled: () => {
			// Always refetch after mutation completes (success or failure)
			// so shortcut errors are up to date
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			queryClient.refetchQueries({ queryKey: ["shortcutErrors"] });
		},
	});
}

// History queries and mutations
export function useHistory(limit?: number) {
	return useQuery({
		queryKey: ["history", limit],
		queryFn: () => tauriAPI.getHistory(limit),
	});
}

export function useAddHistoryEntry() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: ({ text, rawText }: { text: string; rawText: string }) =>
			tauriAPI.addHistoryEntry(text, rawText),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["history"] });
			// Notify other windows about history change
			tauriAPI.emitHistoryChanged();
		},
	});
}

export function useDeleteHistoryEntry() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (id: string) => tauriAPI.deleteHistoryEntry(id),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["history"] });
			// Notify other windows about history change
			tauriAPI.emitHistoryChanged();
		},
	});
}

export function useClearHistory() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: () => tauriAPI.clearHistory(),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["history"] });
			// Notify other windows about history change
			tauriAPI.emitHistoryChanged();
		},
	});
}

// Config API queries and mutations (FastAPI server)
export function useDefaultSections() {
	const { data: serverUrl } = useServerUrl();

	return useQuery({
		queryKey: ["defaultSections", serverUrl],
		queryFn: () => {
			if (!serverUrl) {
				throw new Error("Server URL not available");
			}
			return configAPI.getDefaultSections(serverUrl);
		},
		staleTime: STALE_TIME_INFINITE,
		retry: false, // Don't retry if server not available
		enabled: !!serverUrl, // Only fetch when server URL is available
	});
}

// Provider queries - fetches from HTTP API

/**
 * Hook to fetch available providers from the HTTP API.
 * This is global endpoint (not per-client) since available providers
 * are determined by server configuration (API keys).
 */
export function useAvailableProviders() {
	const { data: serverUrl } = useServerUrl();

	return useQuery<AvailableProvidersData | null>({
		queryKey: ["availableProviders", serverUrl],
		queryFn: async () => {
			if (!serverUrl) return null;
			try {
				return await configAPI.getAvailableProviders(serverUrl);
			} catch {
				// Return null if server not available (will retry on connection)
				return null;
			}
		},
		staleTime: STALE_TIME_PROVIDERS_MS,
		retry: false, // Don't retry, connection handling will refetch
		enabled: !!serverUrl,
	});
}

export function useUpdateSTTTimeout() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (timeoutSeconds: number | null) =>
			tauriAPI.updateSTTTimeout(timeoutSeconds),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			showSettingsSuccess("STT timeout updated successfully");
		},
		onError: (error) => {
			showSettingsError(`Failed to update STT timeout: ${error.message}`);
		},
	});
}

// Server URL mutation
export function useUpdateServerUrl() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (url: string) => tauriAPI.updateServerUrl(url),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			queryClient.invalidateQueries({ queryKey: ["serverUrl"] });
			// Notify other windows about settings change
			tauriAPI.emitSettingsChanged();
			showSettingsSuccess("Server URL updated successfully");
		},
		onError: (error) => {
			showSettingsError(`Failed to update server URL: ${error.message}`);
		},
	});
}

// =============================================================================
// Provider Mutations with Server Confirmation
// =============================================================================
// These hooks wrap the Tauri event-based provider switching in promises,
// implementing pessimistic updates: the UI shows a pending state until
// the server confirms the change succeeded.

// Shared internal function for provider mutations
async function executeProviderChange<TSelection>(
	providerType: "llm" | "stt",
	settingName: "llm-provider" | "stt-provider",
	parseSelection: (value: unknown) => TSelection | null,
	value: string,
	signal?: AbortSignal,
): Promise<TSelection> {
	const { promise, resolve, reject } = Promise.withResolvers<TSelection>();

	// Await listener registration BEFORE emitting to avoid race condition
	const unlisten = await tauriAPI.onConfigResponse((response) => {
		if (response.setting !== settingName) return;

		unlisten();

		if (response.type === "config-updated") {
			const selection = parseSelection(response.value);
			if (selection) resolve(selection);
			else reject(new Error("Failed to parse server response"));
		} else {
			reject(new Error(response.error ?? "Provider change failed"));
		}
	});

	// Clean up listener if aborted (e.g., component unmounts)
	signal?.addEventListener("abort", () => {
		unlisten();
		reject(new DOMException("Aborted", "AbortError"));
	});

	tauriAPI.emitProviderChangeRequest({ providerType, value });

	return promise;
}

function handleProviderMutationSuccess<
	TSelection extends STTProviderSelection | LLMProviderSelection,
>(
	queryClient: QueryClient,
	providerType: "llm" | "stt",
	selection: TSelection | null,
): void {
	if (!selection) return;
	const providerId = getProviderIdFromSelection(selection);

	const { updateTauriSetting, settingsKey } = match(providerType)
		.with("llm", () => ({
			updateTauriSetting: tauriAPI.updateLLMProvider,
			settingsKey: "llm_provider" as const,
		}))
		.with("stt", () => ({
			updateTauriSetting: tauriAPI.updateSTTProvider,
			settingsKey: "stt_provider" as const,
		}))
		.exhaustive();

	updateTauriSetting(providerId);

	// Immediately update the settings cache to prevent flicker
	queryClient.setQueryData(
		["settings"],
		(oldSettings: AppSettings | undefined) => {
			if (!oldSettings) return oldSettings;
			return { ...oldSettings, [settingsKey]: providerId };
		},
	);

	// Invalidate to ensure eventual consistency with server
	queryClient.invalidateQueries({ queryKey: ["settings"] });
}

export function useUpdateLLMProviderWithServer() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: ({ value, signal }: { value: string; signal?: AbortSignal }) =>
			executeProviderChange(
				"llm",
				"llm-provider",
				parseLLMProviderSelection,
				value,
				signal,
			),
		onSuccess: (selection) =>
			handleProviderMutationSuccess(queryClient, "llm", selection),
	});
}

export function useUpdateSTTProviderWithServer() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: ({ value, signal }: { value: string; signal?: AbortSignal }) =>
			executeProviderChange(
				"stt",
				"stt-provider",
				parseSTTProviderSelection,
				value,
				signal,
			),
		onSuccess: (selection) =>
			handleProviderMutationSuccess(queryClient, "stt", selection),
	});
}

// =============================================================================
// Export/Import Hooks
// =============================================================================

/** Parsed file with its detected type and content */
export interface ParsedExportFile {
	type: DetectedFileType | "prompt";
	content: string;
	filename: string;
	/** For prompt files, the section name */
	promptSection?: PromptSectionName;
	/** For prompt files, the actual prompt content (without header) */
	promptContent?: string;
}

/**
 * Hook to export data (settings, history, and prompts) to a selected folder.
 * Opens a folder picker dialog and writes files.
 * - tambourine-settings.json
 * - tambourine-history.json
 * - tambourine-prompt-{section}.md (for each custom prompt)
 */
export function useExportData() {
	return useMutation({
		mutationFn: async () => {
			const { open } = await import("@tauri-apps/plugin-dialog");
			const { writeTextFile } = await import("@tauri-apps/plugin-fs");

			// Open folder picker
			const selectedPath = await open({
				directory: true,
				multiple: false,
				title: "Select Export Folder",
			});

			if (!selectedPath) {
				// User cancelled
				return null;
			}

			// Generate exports
			const [settingsJson, historyJson, promptExports] = await Promise.all([
				tauriAPI.generateSettingsExport(),
				tauriAPI.generateHistoryExport(),
				tauriAPI.generatePromptExports(),
			]);

			// Write JSON files
			const settingsPath = `${selectedPath}/tambourine-settings.json`;
			const historyPath = `${selectedPath}/tambourine-history.json`;

			const writePromises: Promise<void>[] = [
				writeTextFile(settingsPath, settingsJson),
				writeTextFile(historyPath, historyJson),
			];

			// Write prompt .md files (only for custom prompts)
			const promptFiles: string[] = [];
			for (const [section, content] of Object.entries(promptExports)) {
				const promptPath = `${selectedPath}/tambourine-prompt-${section}.md`;
				writePromises.push(writeTextFile(promptPath, content));
				promptFiles.push(promptPath);
			}

			await Promise.all(writePromises);

			return { settingsPath, historyPath, promptFiles };
		},
		onSuccess: (result) => {
			if (result) {
				const promptCount = result.promptFiles.length;
				const promptMsg =
					promptCount > 0 ? ` and ${promptCount} prompt(s)` : "";
				notifications.show({
					title: "Export Complete",
					message: `Settings, history${promptMsg} exported successfully`,
					color: "green",
					autoClose: 3000,
				});
			}
		},
		onError: (error) => {
			notifications.show({
				title: "Export Failed",
				message: error instanceof Error ? error.message : "Unknown error",
				color: "red",
				autoClose: 5000,
			});
		},
	});
}

/**
 * Hook to import data from selected files.
 * Opens a file picker (multi-select), auto-detects file types.
 * - .json files: detected via `type` field (settings or history)
 * - .md files: detected via HTML comment header (prompts)
 * Returns parsed files for the caller to handle (e.g., show strategy modal for history).
 */
export function useImportData() {
	return useMutation({
		mutationFn: async (): Promise<ParsedExportFile[]> => {
			const { open } = await import("@tauri-apps/plugin-dialog");
			const { readTextFile } = await import("@tauri-apps/plugin-fs");

			// Open file picker (allow multiple selection)
			const selectedPaths = await open({
				multiple: true,
				filters: [
					{
						name: "Export Files",
						extensions: ["json", "md"],
					},
				],
				title: "Select Export Files to Import",
			});

			if (!selectedPaths || selectedPaths.length === 0) {
				return [];
			}

			// Read and detect file types
			const files: ParsedExportFile[] = [];

			for (const path of selectedPaths) {
				const content = await readTextFile(path);
				const filename = path.split("/").pop() ?? path;

				// Check if it's a markdown file (potential prompt)
				if (filename.endsWith(".md")) {
					try {
						const [section, promptContent] =
							await tauriAPI.parsePromptFile(content);
						files.push({
							type: "prompt",
							content,
							filename,
							promptSection: section,
							promptContent,
						});
					} catch {
						// Not a valid prompt file, mark as unknown
						files.push({ type: "unknown", content, filename });
					}
				} else {
					// JSON file - detect type from content
					const type = await tauriAPI.detectExportFileType(content);
					files.push({ type, content, filename });
				}
			}

			return files;
		},
	});
}

/**
 * Hook to import settings from a parsed file content.
 * Uses pessimistic sync for providers via overlay.
 */
export function useImportSettings() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: async (content: string) => {
			await tauriAPI.importSettings(content);
			await tauriAPI.registerShortcuts();

			const settings = await tauriAPI.getSettings();

			await executeProviderChange(
				"stt",
				"stt-provider",
				parseSTTProviderSelection,
				settings.stt_provider,
			);

			await executeProviderChange(
				"llm",
				"llm-provider",
				parseLLMProviderSelection,
				settings.llm_provider,
			);
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			queryClient.invalidateQueries({ queryKey: ["shortcutErrors"] });
			tauriAPI.emitSettingsChanged();
			notifications.show({
				title: "Settings Imported",
				message: "Settings have been imported and applied",
				color: "green",
				autoClose: 3000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Import Failed",
				message: error instanceof Error ? error.message : "Unknown error",
				color: "red",
				autoClose: 5000,
			});
		},
	});
}

/**
 * Hook to import history from a parsed file content with a strategy.
 */
export function useImportHistory() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: async ({
			content,
			strategy,
		}: {
			content: string;
			strategy: HistoryImportStrategy;
		}) => {
			return tauriAPI.importHistory(content, strategy);
		},
		onSuccess: (result) => {
			queryClient.invalidateQueries({ queryKey: ["history"] });
			tauriAPI.emitHistoryChanged();
			const imported = result.entries_imported ?? 0;
			notifications.show({
				title: "History Imported",
				message: `${imported} entries imported`,
				color: "green",
				autoClose: 3000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Import Failed",
				message: error instanceof Error ? error.message : "Unknown error",
				color: "red",
				autoClose: 5000,
			});
		},
	});
}

/**
 * Hook to import a prompt from a parsed file content.
 */
export function useImportPrompt() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: async ({
			section,
			content,
		}: {
			section: PromptSectionName;
			content: string;
		}) => {
			return tauriAPI.importPrompt(section, content);
		},
		onSuccess: (_result, { section }) => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			notifications.show({
				title: "Prompt Imported",
				message: `${section} prompt imported successfully`,
				color: "green",
				autoClose: 3000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Import Failed",
				message: error instanceof Error ? error.message : "Unknown error",
				color: "red",
				autoClose: 5000,
			});
		},
	});
}

/**
 * Hook to perform a factory reset.
 * Uses pessimistic sync for default providers via overlay.
 */
export function useFactoryReset() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: async () => {
			await tauriAPI.factoryReset();
			await tauriAPI.registerShortcuts();

			const defaultProvider = "auto";

			await executeProviderChange(
				"stt",
				"stt-provider",
				parseSTTProviderSelection,
				defaultProvider,
			);

			await executeProviderChange(
				"llm",
				"llm-provider",
				parseLLMProviderSelection,
				defaultProvider,
			);
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			queryClient.invalidateQueries({ queryKey: ["history"] });
			queryClient.invalidateQueries({ queryKey: ["shortcutErrors"] });
			tauriAPI.emitSettingsChanged();
			tauriAPI.emitHistoryChanged();
			notifications.show({
				title: "Factory Reset Complete",
				message: "All settings and history have been reset to defaults",
				color: "green",
				autoClose: 3000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Factory Reset Failed",
				message: error instanceof Error ? error.message : "Unknown error",
				color: "red",
				autoClose: 5000,
			});
		},
	});
}
