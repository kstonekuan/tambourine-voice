import { Select } from "@mantine/core";
import { useEffect, useState } from "react";
import { useSettings, useUpdateSelectedMic } from "../lib/queries";

interface AudioDevice {
	deviceId: string;
	label: string;
}

export function DeviceSelector() {
	const { data: settings, isLoading: settingsLoading } = useSettings();
	const updateSelectedMic = useUpdateSelectedMic();
	const [devices, setDevices] = useState<AudioDevice[]>([]);
	const [isLoading, setIsLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		async function loadDevices() {
			try {
				// Request permission first (needed to get device labels)
				const stream = await navigator.mediaDevices.getUserMedia({
					audio: true,
				});

				// Stop the audio track immediately to release the microphone
				for (const track of stream.getTracks()) {
					track.stop();
				}

				const allDevices = await navigator.mediaDevices.enumerateDevices();
				const audioInputs = allDevices
					.filter((device) => device.kind === "audioinput")
					.map((device) => ({
						deviceId: device.deviceId,
						label: device.label || `Microphone ${device.deviceId.slice(0, 8)}`,
					}));

				setDevices(audioInputs);
				setError(null);
			} catch (err) {
				setError("Could not access microphones. Please grant permission.");
				console.error("Failed to enumerate devices:", err);
			} finally {
				setIsLoading(false);
			}
		}

		loadDevices();

		// Listen for device changes
		const handleDeviceChange = () => {
			loadDevices();
		};
		navigator.mediaDevices.addEventListener("devicechange", handleDeviceChange);

		return () => {
			navigator.mediaDevices.removeEventListener(
				"devicechange",
				handleDeviceChange,
			);
		};
	}, []);

	const handleChange = (value: string | null) => {
		// null or empty string means "default"
		const micId = value === "" || value === "default" ? null : value;
		updateSelectedMic.mutate(micId);
	};

	if (isLoading || settingsLoading) {
		return (
			<div>
				<p className="settings-label">Microphone</p>
				<p className="settings-description">Loading microphones...</p>
			</div>
		);
	}

	if (error) {
		return (
			<div>
				<p className="settings-label">Microphone</p>
				<p className="settings-description" style={{ color: "#ef4444" }}>
					{error}
				</p>
			</div>
		);
	}

	const selectData = [
		{ value: "default", label: "System Default" },
		...devices
			.filter((device) => device.deviceId !== "default")
			.map((device) => ({
				value: device.deviceId,
				label: device.label,
			})),
	];

	return (
		<Select
			label="Microphone"
			description="Select which microphone to use for dictation"
			data={selectData}
			value={settings?.selected_mic_id ?? "default"}
			onChange={handleChange}
			allowDeselect={false}
			className="device-selector"
		/>
	);
}
