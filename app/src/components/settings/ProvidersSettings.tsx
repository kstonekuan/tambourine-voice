import { Badge, Loader, Select, Slider, Text } from "@mantine/core";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
	useAvailableProviders,
	useSettings,
	useUpdateLLMProvider,
	useUpdateSTTProvider,
	useUpdateSTTTimeout,
} from "../../lib/queries";
import { tauriAPI } from "../../lib/tauri";

const DEFAULT_STT_TIMEOUT = 0.8;

export function ProvidersSettings() {
	const { data: settings, isLoading: isLoadingSettings } = useSettings();
	const { data: availableProviders, isLoading: isLoadingProviders } =
		useAvailableProviders();

	// Wait for settings (source of truth) and provider list (for options)
	const isLoadingProviderData = isLoadingSettings || isLoadingProviders;
	const updateSTTProvider = useUpdateSTTProvider();
	const updateLLMProvider = useUpdateLLMProvider();
	const updateSTTTimeout = useUpdateSTTTimeout();

	const handleSTTProviderChange = useCallback(
		(value: string | null) => {
			if (!value) return;
			// Save to Tauri, then notify overlay to sync via RTVI
			updateSTTProvider.mutate(value, {
				onSuccess: () => {
					tauriAPI.emitSettingsChanged();
				},
			});
		},
		[updateSTTProvider],
	);

	const handleLLMProviderChange = useCallback(
		(value: string | null) => {
			if (!value) return;
			// Save to Tauri, then notify overlay to sync via RTVI
			updateLLMProvider.mutate(value, {
				onSuccess: () => {
					tauriAPI.emitSettingsChanged();
				},
			});
		},
		[updateLLMProvider],
	);

	const handleSTTTimeoutChange = useCallback(
		(value: number) => {
			// Save to Tauri, which syncs to server
			updateSTTTimeout.mutate(value);
		},
		[updateSTTTimeout],
	);

	// Get the current timeout value from settings, falling back to default
	const currentTimeout = settings?.stt_timeout_seconds ?? DEFAULT_STT_TIMEOUT;

	// Local state for smooth slider dragging
	const [sliderValue, setSliderValue] = useState(currentTimeout);

	// Sync local state when server value changes
	useEffect(() => {
		setSliderValue(currentTimeout);
	}, [currentTimeout]);

	// Group providers by cloud/local for dropdown display (memoized to prevent unnecessary re-renders)
	const sttCloudProviders = useMemo(
		() =>
			availableProviders?.stt
				.filter((p) => !p.is_local)
				.map((p) => ({
					value: p.value,
					label: p.model ? `${p.label} (${p.model})` : p.label,
				})) ?? [],
		[availableProviders],
	);
	const sttLocalProviders = useMemo(
		() =>
			availableProviders?.stt
				.filter((p) => p.is_local)
				.map((p) => ({
					value: p.value,
					label: p.model ? `${p.label} (${p.model})` : p.label,
				})) ?? [],
		[availableProviders],
	);
	const sttProviderOptions = useMemo(
		() => [
			{ group: "", items: [{ value: "auto", label: "Auto" }] },
			{ group: "Cloud", items: sttCloudProviders },
			{ group: "Local", items: sttLocalProviders },
		],
		[sttCloudProviders, sttLocalProviders],
	);

	const llmCloudProviders = useMemo(
		() =>
			availableProviders?.llm
				.filter((p) => !p.is_local)
				.map((p) => ({
					value: p.value,
					label: p.model ? `${p.label} (${p.model})` : p.label,
				})) ?? [],
		[availableProviders],
	);
	const llmLocalProviders = useMemo(
		() =>
			availableProviders?.llm
				.filter((p) => p.is_local)
				.map((p) => ({
					value: p.value,
					label: p.model ? `${p.label} (${p.model})` : p.label,
				})) ?? [],
		[availableProviders],
	);
	const llmProviderOptions = useMemo(
		() => [
			{ group: "", items: [{ value: "auto", label: "Auto" }] },
			{ group: "Cloud", items: llmCloudProviders },
			{ group: "Local", items: llmLocalProviders },
		],
		[llmCloudProviders, llmLocalProviders],
	);

	// Determine if currently selected provider is local (only show badge for non-auto providers)
	const selectedSttProvider = availableProviders?.stt.find(
		(p) => p.value === settings?.stt_provider,
	);
	const selectedLlmProvider = availableProviders?.llm.find(
		(p) => p.value === settings?.llm_provider,
	);
	const isSttProviderAuto = settings?.stt_provider === "auto";
	const isLlmProviderAuto = settings?.llm_provider === "auto";
	const isSttProviderLocal = selectedSttProvider?.is_local ?? false;
	const isLlmProviderLocal = selectedLlmProvider?.is_local ?? false;

	return (
		<div className="settings-section animate-in animate-in-delay-1">
			<h3 className="settings-section-title">Providers</h3>
			<div className="settings-card">
				<div className="settings-row">
					<div>
						<p className="settings-label">Speech-to-Text (STT)</p>
						<p className="settings-description">
							Service for transcribing audio
						</p>
					</div>
					<div style={{ display: "flex", alignItems: "center", gap: 8 }}>
						{isLoadingProviderData ? (
							<Loader size="sm" color="gray" />
						) : (
							<>
								<Select
									data={sttProviderOptions}
									value={settings?.stt_provider ?? "auto"}
									onChange={handleSTTProviderChange}
									placeholder="Select provider"
									disabled={
										sttCloudProviders.length === 0 &&
										sttLocalProviders.length === 0
									}
									styles={{
										input: {
											backgroundColor: "var(--bg-elevated)",
											borderColor: "var(--border-default)",
											color: "var(--text-primary)",
										},
									}}
								/>
								{!isSttProviderAuto && settings?.stt_provider && (
									<Badge
										size="xs"
										variant="light"
										color={isSttProviderLocal ? "teal" : "blue"}
									>
										{isSttProviderLocal ? "Local" : "Cloud"}
									</Badge>
								)}
							</>
						)}
					</div>
				</div>
				<div className="settings-row" style={{ marginTop: 16 }}>
					<div>
						<p className="settings-label">Large Language Model (LLM)</p>
						<p className="settings-description">Service for text formatting</p>
					</div>
					<div style={{ display: "flex", alignItems: "center", gap: 8 }}>
						{isLoadingProviderData ? (
							<Loader size="sm" color="gray" />
						) : (
							<>
								<Select
									data={llmProviderOptions}
									value={settings?.llm_provider ?? "auto"}
									onChange={handleLLMProviderChange}
									placeholder="Select provider"
									disabled={
										llmCloudProviders.length === 0 &&
										llmLocalProviders.length === 0
									}
									styles={{
										input: {
											backgroundColor: "var(--bg-elevated)",
											borderColor: "var(--border-default)",
											color: "var(--text-primary)",
										},
									}}
								/>
								{!isLlmProviderAuto && settings?.llm_provider && (
									<Badge
										size="xs"
										variant="light"
										color={isLlmProviderLocal ? "teal" : "blue"}
									>
										{isLlmProviderLocal ? "Local" : "Cloud"}
									</Badge>
								)}
							</>
						)}
					</div>
				</div>
				<div className="settings-row" style={{ marginTop: 16 }}>
					<div style={{ flex: 1 }}>
						<p className="settings-label">STT Timeout</p>
						<p className="settings-description">
							Increase if nothing is getting transcribed
						</p>
						<div
							style={{
								marginTop: 12,
								display: "flex",
								alignItems: "center",
								gap: 12,
							}}
						>
							<Slider
								value={sliderValue}
								onChange={setSliderValue}
								onChangeEnd={handleSTTTimeoutChange}
								min={0.5}
								max={3.0}
								step={0.1}
								marks={[
									{ value: 0.5, label: "0.5s" },
									{ value: 3.0, label: "3.0s" },
								]}
								styles={{
									root: { flex: 1 },
									track: { backgroundColor: "var(--bg-elevated)" },
									bar: { backgroundColor: "var(--accent-primary)" },
									thumb: { borderColor: "var(--accent-primary)" },
									markLabel: { color: "var(--text-secondary)", fontSize: 10 },
								}}
							/>
							<Text size="xs" c="dimmed" style={{ minWidth: 32 }}>
								{sliderValue.toFixed(1)}s
							</Text>
						</div>
					</div>
				</div>
			</div>
		</div>
	);
}
