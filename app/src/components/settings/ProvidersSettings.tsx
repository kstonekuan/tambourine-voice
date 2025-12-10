import { Loader, Select, Slider, Text } from "@mantine/core";
import { useEffect, useState } from "react";
import {
	useAvailableProviders,
	useCurrentProviders,
	useSetServerLLMProvider,
	useSetServerSTTProvider,
	useSetServerSTTTimeout,
	useSettings,
	useSTTTimeout,
	useUpdateLLMProvider,
	useUpdateSTTProvider,
	useUpdateSTTTimeout,
} from "../../lib/queries";

const DEFAULT_STT_TIMEOUT = 0.8;

export function ProvidersSettings() {
	const { data: settings, isLoading: isLoadingSettings } = useSettings();
	const { data: availableProviders, isLoading: isLoadingProviders } =
		useAvailableProviders();
	const { data: currentProviders } = useCurrentProviders();

	// Wait for settings (source of truth) and provider list (for options)
	// currentProviders is only used as fallback if settings doesn't have a value
	const isLoadingProviderData = isLoadingSettings || isLoadingProviders;
	const { data: sttTimeout } = useSTTTimeout();
	const updateSTTProvider = useUpdateSTTProvider();
	const updateLLMProvider = useUpdateLLMProvider();
	const setServerSTTProvider = useSetServerSTTProvider();
	const setServerLLMProvider = useSetServerLLMProvider();
	const updateSTTTimeout = useUpdateSTTTimeout();
	const setServerSTTTimeout = useSetServerSTTTimeout();

	const handleSTTProviderChange = (value: string | null) => {
		if (!value) return;
		updateSTTProvider.mutate(value, {
			onSuccess: () => {
				setServerSTTProvider.mutate(value);
			},
		});
	};

	const handleLLMProviderChange = (value: string | null) => {
		if (!value) return;
		updateLLMProvider.mutate(value, {
			onSuccess: () => {
				setServerLLMProvider.mutate(value);
			},
		});
	};

	const handleSTTTimeoutChange = (value: number) => {
		updateSTTTimeout.mutate(value, {
			onSuccess: () => {
				setServerSTTTimeout.mutate(value);
			},
		});
	};

	// Get the current timeout value from server or settings, falling back to default
	const currentTimeout =
		sttTimeout?.timeout_seconds ??
		settings?.stt_timeout_seconds ??
		DEFAULT_STT_TIMEOUT;

	// Local state for smooth slider dragging
	const [sliderValue, setSliderValue] = useState(currentTimeout);

	// Sync local state when server value changes
	useEffect(() => {
		setSliderValue(currentTimeout);
	}, [currentTimeout]);

	const sttProviderOptions =
		availableProviders?.stt.map((p) => ({
			value: p.value,
			label: p.label,
		})) ?? [];

	const llmProviderOptions =
		availableProviders?.llm.map((p) => ({
			value: p.value,
			label: p.label,
		})) ?? [];

	return (
		<div className="settings-section animate-in animate-in-delay-1">
			<h3 className="settings-section-title">Providers</h3>
			<div className="settings-card">
				<div className="settings-row">
					<div>
						<p className="settings-label">Speech-to-Text</p>
						<p className="settings-description">
							Service for transcribing audio
						</p>
					</div>
					{isLoadingProviderData ? (
						<Loader size="sm" color="gray" />
					) : (
						<Select
							data={sttProviderOptions}
							value={settings?.stt_provider ?? currentProviders?.stt ?? null}
							onChange={handleSTTProviderChange}
							placeholder="Select provider"
							disabled={sttProviderOptions.length === 0}
							styles={{
								input: {
									backgroundColor: "var(--bg-elevated)",
									borderColor: "var(--border-default)",
									color: "var(--text-primary)",
								},
							}}
						/>
					)}
				</div>
				<div className="settings-row" style={{ marginTop: 16 }}>
					<div>
						<p className="settings-label">Language Model</p>
						<p className="settings-description">AI service for text cleanup</p>
					</div>
					{isLoadingProviderData ? (
						<Loader size="sm" color="gray" />
					) : (
						<Select
							data={llmProviderOptions}
							value={settings?.llm_provider ?? currentProviders?.llm ?? null}
							onChange={handleLLMProviderChange}
							placeholder="Select provider"
							disabled={llmProviderOptions.length === 0}
							styles={{
								input: {
									backgroundColor: "var(--bg-elevated)",
									borderColor: "var(--border-default)",
									color: "var(--text-primary)",
								},
							}}
						/>
					)}
				</div>
				<div className="settings-row" style={{ marginTop: 16 }}>
					<div style={{ flex: 1 }}>
						<p className="settings-label">STT Timeout</p>
						<p className="settings-description">
							Increase if nothing is getting transcribed
						</p>
						<div style={{ marginTop: 12, paddingRight: 8 }}>
							<Slider
								value={sliderValue}
								onChange={setSliderValue}
								onChangeEnd={handleSTTTimeoutChange}
								min={0.5}
								max={3.0}
								step={0.1}
								marks={[
									{ value: 0.5, label: "0.5s" },
									{ value: 1.5, label: "1.5s" },
									{ value: 3.0, label: "3.0s" },
								]}
								styles={{
									track: { backgroundColor: "var(--bg-elevated)" },
									bar: { backgroundColor: "var(--accent-primary)" },
									thumb: { borderColor: "var(--accent-primary)" },
									markLabel: { color: "var(--text-secondary)", fontSize: 10 },
								}}
							/>
							<Text size="xs" c="dimmed" ta="center" style={{ marginTop: 8 }}>
								{sliderValue.toFixed(1)}s
							</Text>
						</div>
					</div>
				</div>
			</div>
		</div>
	);
}
