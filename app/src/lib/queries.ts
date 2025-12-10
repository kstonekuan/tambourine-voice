import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";
import {
	type CleanupPromptSections,
	configAPI,
	type HotkeyConfig,
	tauriAPI,
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
			// Invalidate all server-side queries
			queryClient.invalidateQueries({ queryKey: ["availableProviders"] });
			queryClient.invalidateQueries({ queryKey: ["currentProviders"] });
			queryClient.invalidateQueries({ queryKey: ["sttTimeout"] });
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
		mutationFn: (hotkey: HotkeyConfig) =>
			tauriAPI.updateToggleHotkeyLive(hotkey),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
		},
	});
}

export function useUpdateHoldHotkey() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (hotkey: HotkeyConfig) => tauriAPI.updateHoldHotkeyLive(hotkey),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["settings"] });
		},
	});
}

export function useUpdatePasteLastHotkey() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (hotkey: HotkeyConfig) =>
			tauriAPI.updatePasteLastHotkeyLive(hotkey),
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
		mutationFn: () => tauriAPI.resetHotkeysToDefaults(),
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
	return useQuery({
		queryKey: ["defaultSections"],
		queryFn: () => configAPI.getDefaultSections(),
		staleTime: Number.POSITIVE_INFINITY, // Default prompts never change
		retry: false, // Don't retry if server not available
	});
}

export function useSetServerPromptSections() {
	return useMutation({
		mutationFn: (sections: CleanupPromptSections) =>
			configAPI.setPromptSections(sections),
	});
}

// Provider queries and mutations

export function useAvailableProviders() {
	return useQuery({
		queryKey: ["availableProviders"],
		queryFn: () => configAPI.getAvailableProviders(),
		retry: false, // Don't retry if server not available
	});
}

export function useCurrentProviders() {
	return useQuery({
		queryKey: ["currentProviders"],
		queryFn: () => configAPI.getCurrentProviders(),
		retry: false,
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

export function useSetServerSTTProvider() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (provider: string) => configAPI.setSTTProvider(provider),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["currentProviders"] });
		},
	});
}

export function useSetServerLLMProvider() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (provider: string) => configAPI.setLLMProvider(provider),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["currentProviders"] });
		},
	});
}

// STT Timeout queries and mutations
export function useSTTTimeout() {
	return useQuery({
		queryKey: ["sttTimeout"],
		queryFn: () => configAPI.getSTTTimeout(),
		retry: false,
	});
}

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

export function useSetServerSTTTimeout() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (timeoutSeconds: number) =>
			configAPI.setSTTTimeout(timeoutSeconds),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ["sttTimeout"] });
		},
	});
}
