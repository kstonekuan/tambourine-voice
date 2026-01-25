import { notifications } from "@mantine/notifications";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";
import {
	type AvailableProvidersData,
	type CleanupPromptSections,
	type ConnectionState,
	configAPI,
	getProviderIdFromSelection,
	type HotkeyConfig,
	parseLLMProviderSelection,
	parseSTTProviderSelection,
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
		staleTime: Number.POSITIVE_INFINITY,
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
		staleTime: Number.POSITIVE_INFINITY,
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
			notifications.show({
				title: "Settings Updated",
				message: "Toggle hotkey updated successfully",
				color: "green",
				autoClose: 2000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Settings Error",
				message: `Failed to update toggle hotkey: ${error.message}`,
				color: "red",
				autoClose: 5000,
			});
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
			notifications.show({
				title: "Settings Updated",
				message: "Hold hotkey updated successfully",
				color: "green",
				autoClose: 2000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Settings Error",
				message: `Failed to update hold hotkey: ${error.message}`,
				color: "red",
				autoClose: 5000,
			});
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
			notifications.show({
				title: "Settings Updated",
				message: "Paste last hotkey updated successfully",
				color: "green",
				autoClose: 2000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Settings Error",
				message: `Failed to update paste last hotkey: ${error.message}`,
				color: "red",
				autoClose: 5000,
			});
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
			notifications.show({
				title: "Settings Updated",
				message: "Microphone selection updated successfully",
				color: "green",
				autoClose: 2000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Settings Error",
				message: `Failed to update microphone: ${error.message}`,
				color: "red",
				autoClose: 5000,
			});
		},
	});
}

export function useUpdateSoundEnabled() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (enabled: boolean) => tauriAPI.updateSoundEnabled(enabled),
		onSuccess: (_data, enabled) => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			notifications.show({
				title: "Settings Updated",
				message: `Sound feedback ${enabled ? "enabled" : "disabled"}`,
				color: "green",
				autoClose: 2000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Settings Error",
				message: `Failed to update sound setting: ${error.message}`,
				color: "red",
				autoClose: 5000,
			});
		},
	});
}

export function useUpdateAutoMuteAudio() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (enabled: boolean) => tauriAPI.updateAutoMuteAudio(enabled),
		onSuccess: (_data, enabled) => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			notifications.show({
				title: "Settings Updated",
				message: `Auto-mute ${enabled ? "enabled" : "disabled"}`,
				color: "green",
				autoClose: 2000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Settings Error",
				message: `Failed to update auto-mute setting: ${error.message}`,
				color: "red",
				autoClose: 5000,
			});
		},
	});
}

export function useIsAudioMuteSupported() {
	return useQuery({
		queryKey: ["audioMuteSupported"],
		queryFn: () => tauriAPI.isAudioMuteSupported(),
		staleTime: Number.POSITIVE_INFINITY,
	});
}

export function useUpdateCleanupPromptSections() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (sections: CleanupPromptSections | null) =>
			tauriAPI.updateCleanupPromptSections(sections),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
			notifications.show({
				title: "Settings Updated",
				message: "Formatting prompt updated successfully",
				color: "green",
				autoClose: 2000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Settings Error",
				message: `Failed to update formatting prompt: ${error.message}`,
				color: "red",
				autoClose: 5000,
			});
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
			notifications.show({
				title: "Settings Updated",
				message: "Hotkeys reset to defaults",
				color: "green",
				autoClose: 2000,
			});
		},
		onError: (error) => {
			console.error("Reset hotkeys failed:", error);
			notifications.show({
				title: "Settings Error",
				message: `Failed to reset hotkeys: ${error.message}`,
				color: "red",
				autoClose: 5000,
			});
		},
	});
}

export function useShortcutErrors() {
	return useQuery({
		queryKey: ["shortcutErrors"],
		queryFn: () => tauriAPI.getShortcutErrors(),
		staleTime: 0, // Always refetch to get the latest errors
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

			// If enabling failed, throw an error so the UI can show it
			if (enabled) {
				const errorKey = `${hotkeyType}_error` as keyof typeof result.errors;
				const registeredKey = `${hotkeyType}_registered` as keyof typeof result;
				if (!result[registeredKey] && result.errors[errorKey]) {
					throw new Error(result.errors[errorKey] as string);
				}
			}

			return { result, hotkeyType, enabled };
		},
		onSuccess: ({ hotkeyType, enabled }) => {
			const label = HOTKEY_TYPE_LABELS[hotkeyType];
			notifications.show({
				title: "Settings Updated",
				message: `${label} hotkey ${enabled ? "enabled" : "disabled"}`,
				color: "green",
				autoClose: 2000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Settings Error",
				message: `Failed to update hotkey: ${error.message}`,
				color: "red",
				autoClose: 5000,
			});
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
		mutationFn: (text: string) => tauriAPI.addHistoryEntry(text),
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
		staleTime: Number.POSITIVE_INFINITY, // Default prompts never change
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
		staleTime: 30_000, // Consider stale after 30 seconds
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
			notifications.show({
				title: "Settings Updated",
				message: "STT timeout updated successfully",
				color: "green",
				autoClose: 2000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Settings Error",
				message: `Failed to update STT timeout: ${error.message}`,
				color: "red",
				autoClose: 5000,
			});
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
			notifications.show({
				title: "Settings Updated",
				message: "Server URL updated successfully",
				color: "green",
				autoClose: 2000,
			});
		},
		onError: (error) => {
			notifications.show({
				title: "Settings Error",
				message: `Failed to update server URL: ${error.message}`,
				color: "red",
				autoClose: 5000,
			});
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

	tauriAPI.emitProviderChangeRequest({ providerType, value });

	return promise;
}

export function useUpdateLLMProviderWithServer() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (value: string) =>
			executeProviderChange(
				"llm",
				"llm-provider",
				parseLLMProviderSelection,
				value,
			),
		onSuccess: (selection) => {
			if (!selection) return;
			const providerId = getProviderIdFromSelection(selection);
			tauriAPI.updateLLMProvider(providerId);
			queryClient.invalidateQueries({ queryKey: ["settings"] });
		},
	});
}

export function useUpdateSTTProviderWithServer() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (value: string) =>
			executeProviderChange(
				"stt",
				"stt-provider",
				parseSTTProviderSelection,
				value,
			),
		onSuccess: (selection) => {
			if (!selection) return;
			const providerId = getProviderIdFromSelection(selection);
			tauriAPI.updateSTTProvider(providerId);
			queryClient.invalidateQueries({ queryKey: ["settings"] });
		},
	});
}
