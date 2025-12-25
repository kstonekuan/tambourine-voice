import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";
import {
	type AvailableProvidersData,
	type CleanupPromptSections,
	configAPI,
	type HotkeyConfig,
	tauriAPI,
	validateHotkeyNotDuplicate,
} from "./tauri";

type ConnectionState =
	| "disconnected"
	| "connecting"
	| "idle"
	| "recording"
	| "processing";

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
		const wasDisconnected =
			previousStateRef.current === "disconnected" ||
			previousStateRef.current === "connecting";
		const isNowConnected =
			connectionState === "idle" ||
			connectionState === "recording" ||
			connectionState === "processing";

		if (wasDisconnected && isNowConnected) {
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

export function useUpdateToggleHotkey() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: async (hotkey: HotkeyConfig) => {
			// Get current settings for validation
			const settings = await tauriAPI.getSettings();

			// Validate no duplicate
			const error = validateHotkeyNotDuplicate(
				hotkey,
				{
					toggle: settings.toggle_hotkey,
					hold: settings.hold_hotkey,
					paste_last: settings.paste_last_hotkey,
				},
				"toggle",
			);
			if (error) throw new Error(error);

			// Save and re-register
			await tauriAPI.updateToggleHotkey(hotkey);
			await tauriAPI.registerShortcuts();
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
		},
	});
}

export function useUpdateHoldHotkey() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: async (hotkey: HotkeyConfig) => {
			// Get current settings for validation
			const settings = await tauriAPI.getSettings();

			// Validate no duplicate
			const error = validateHotkeyNotDuplicate(
				hotkey,
				{
					toggle: settings.toggle_hotkey,
					hold: settings.hold_hotkey,
					paste_last: settings.paste_last_hotkey,
				},
				"hold",
			);
			if (error) throw new Error(error);

			// Save and re-register
			await tauriAPI.updateHoldHotkey(hotkey);
			await tauriAPI.registerShortcuts();
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
		},
	});
}

export function useUpdatePasteLastHotkey() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: async (hotkey: HotkeyConfig) => {
			// Get current settings for validation
			const settings = await tauriAPI.getSettings();

			// Validate no duplicate
			const error = validateHotkeyNotDuplicate(
				hotkey,
				{
					toggle: settings.toggle_hotkey,
					hold: settings.hold_hotkey,
					paste_last: settings.paste_last_hotkey,
				},
				"paste_last",
			);
			if (error) throw new Error(error);

			// Save and re-register
			await tauriAPI.updatePasteLastHotkey(hotkey);
			await tauriAPI.registerShortcuts();
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
		},
	});
}

export function useUpdateSelectedMic() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (micId: string | null) => tauriAPI.updateSelectedMic(micId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
		},
	});
}

export function useUpdateSoundEnabled() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (enabled: boolean) => tauriAPI.updateSoundEnabled(enabled),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
		},
	});
}

export function useUpdateAutoMuteAudio() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (enabled: boolean) => tauriAPI.updateAutoMuteAudio(enabled),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
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
		},
		onError: (error) => {
			console.error("Reset hotkeys failed:", error);
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

// Provider queries - data comes from RTVI message via Tauri event

/**
 * Hook to set up the available providers event listener.
 * Call this from a component that stays mounted (like App.tsx) to ensure
 * the listener is always active and data is cached properly.
 */
export function useAvailableProvidersListener() {
	const queryClient = useQueryClient();

	// Listen for provider data from overlay window (relayed from server via RTVI)
	useEffect(() => {
		const unlistenPromise = tauriAPI.onAvailableProviders((data) => {
			queryClient.setQueryData<AvailableProvidersData>(
				["availableProviders"],
				data,
			);
		});

		return () => {
			unlistenPromise.then((unlisten) => unlisten());
		};
	}, [queryClient]);
}

/**
 * Hook to read available providers from the cache.
 * The data is populated by useAvailableProvidersListener which should be
 * called from a parent component that stays mounted.
 */
export function useAvailableProviders() {
	return useQuery<AvailableProvidersData | null>({
		queryKey: ["availableProviders"],
		queryFn: () => Promise.resolve(null), // No initial fetch, data comes from event
		staleTime: Number.POSITIVE_INFINITY,
		enabled: false, // Don't auto-fetch
	});
}

export function useUpdateSTTProvider() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (provider: string | null) =>
			tauriAPI.updateSTTProvider(provider),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
		},
	});
}

export function useUpdateLLMProvider() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (provider: string | null) =>
			tauriAPI.updateLLMProvider(provider),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
		},
	});
}

// STT Timeout mutation (local settings)
export function useUpdateSTTTimeout() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (timeoutSeconds: number | null) =>
			tauriAPI.updateSTTTimeout(timeoutSeconds),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
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
		},
	});
}
