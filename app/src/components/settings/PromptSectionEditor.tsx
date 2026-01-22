import {
	Accordion,
	Button,
	SegmentedControl,
	Switch,
	Text,
	Textarea,
} from "@mantine/core";
import { useEffect, useState } from "react";

export interface PromptSectionEditorProps {
	sectionKey: string;
	title: string;
	description: string;
	enabled: boolean;
	initialContent: string;
	defaultContent: string;
	hasCustom: boolean;
	helpText?: string;
	placeholder?: string;
	resetLabel?: string;
	minRows?: number;
	maxRows?: number;
	hideToggle?: boolean;
	/** Whether auto mode is enabled (use default prompt) */
	auto?: boolean;
	/** Callback when auto mode is toggled */
	onAutoToggle?: (auto: boolean) => void;
	/** Whether to show the Auto/Manual toggle (default: false) */
	showAutoToggle?: boolean;
	onToggle: (enabled: boolean) => void;
	onSave: (content: string) => void;
	onReset: () => void;
	isSaving: boolean;
}

export function PromptSectionEditor({
	sectionKey,
	title,
	description,
	enabled,
	initialContent,
	defaultContent,
	hasCustom,
	helpText,
	placeholder,
	resetLabel = "Reset to Default",
	minRows = 6,
	maxRows = 15,
	hideToggle = false,
	auto = false,
	onAutoToggle,
	showAutoToggle = false,
	onToggle,
	onSave,
	onReset,
	isSaving,
}: PromptSectionEditorProps) {
	const [content, setContent] = useState(initialContent);
	const [hasChanges, setHasChanges] = useState(false);

	// Sync local content when initialContent changes (e.g., after reset)
	useEffect(() => {
		setContent(initialContent);
		setHasChanges(false);
	}, [initialContent]);

	const handleContentChange = (value: string) => {
		setContent(value);
		setHasChanges(true);
	};

	const handleSave = () => {
		onSave(content);
		setHasChanges(false);
	};

	const handleReset = () => {
		setContent(defaultContent);
		onReset();
		setHasChanges(false);
	};

	return (
		<Accordion.Item value={sectionKey}>
			<Accordion.Control>
				<div
					style={{
						display: "flex",
						justifyContent: "space-between",
						alignItems: "center",
						width: "100%",
						paddingRight: 12,
					}}
				>
					<div>
						<p className="settings-label">{title}</p>
						<p className="settings-description">{description}</p>
					</div>
					{!hideToggle && (
						<Switch
							checked={enabled}
							onChange={(e) => {
								e.stopPropagation();
								onToggle(e.currentTarget.checked);
							}}
							onClick={(e) => e.stopPropagation()}
							color="gray"
							size="md"
						/>
					)}
				</div>
			</Accordion.Control>
			<Accordion.Panel>
				{showAutoToggle && (
					<SegmentedControl
						value={auto ? "auto" : "manual"}
						onChange={(value) => onAutoToggle?.(value === "auto")}
						data={[
							{ label: "Auto", value: "auto" },
							{ label: "Manual", value: "manual" },
						]}
						size="xs"
						mb="md"
						styles={{
							root: {
								backgroundColor: "var(--bg-elevated)",
							},
						}}
					/>
				)}
				{auto ? (
					<Text size="sm" c="dimmed">
						The prompt is being optimized for you
					</Text>
				) : (
					<>
						{helpText && (
							<Text size="xs" c="dimmed" mb="sm">
								{helpText}
							</Text>
						)}
						<Textarea
							value={content}
							onChange={(e) => handleContentChange(e.currentTarget.value)}
							placeholder={placeholder}
							minRows={minRows}
							maxRows={maxRows}
							autosize
							disabled={!enabled}
							styles={{
								input: {
									backgroundColor: "var(--bg-elevated)",
									borderColor: "var(--border-default)",
									color: "var(--text-primary)",
									fontFamily: "monospace",
									fontSize: "13px",
								},
							}}
						/>
						<div
							style={{
								display: "flex",
								gap: 12,
								marginTop: 16,
								justifyContent: "flex-end",
							}}
						>
							<Button
								variant="subtle"
								color="gray"
								onClick={handleReset}
								disabled={!enabled || !hasCustom}
							>
								{resetLabel}
							</Button>
							<Button
								color="gray"
								onClick={handleSave}
								disabled={!hasChanges}
								loading={isSaving}
							>
								Save
							</Button>
						</div>
					</>
				)}
			</Accordion.Panel>
		</Accordion.Item>
	);
}
