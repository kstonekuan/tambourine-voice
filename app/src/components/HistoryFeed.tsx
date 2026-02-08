import { ActionIcon, Button, Group, Menu, Modal, Text } from "@mantine/core";
import { useClipboard, useDisclosure } from "@mantine/hooks";
import { useQueryClient } from "@tanstack/react-query";
import { format, isToday, isYesterday } from "date-fns";
import {
	Copy,
	Eye,
	EyeOff,
	MessageSquare,
	MoreVertical,
	Trash2,
} from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useState } from "react";
import {
	useClearHistory,
	useDeleteHistoryEntry,
	useHistory,
} from "../lib/queries";
import type { HistoryEntry } from "../lib/tauri";
import { tauriAPI } from "../lib/tauri";

function formatTime(timestamp: string): string {
	return format(new Date(timestamp), "h:mm a");
}

function formatDate(timestamp: string): string {
	const date = new Date(timestamp);
	if (isToday(date)) return "Today";
	if (isYesterday(date)) return "Yesterday";
	return format(date, "MMM d");
}

function normalizeMeaningfulContextValue(
	rawContextValue: string | null | undefined,
): string | null {
	const trimmedContextValue = rawContextValue?.trim();
	return trimmedContextValue ? trimmedContextValue : null;
}

interface GroupedHistory {
	date: string;
	items: HistoryEntry[];
}

function groupHistoryByDate(history: HistoryEntry[]): GroupedHistory[] {
	const groups: Record<string, GroupedHistory> = {};

	for (const item of history) {
		const dateKey = formatDate(item.timestamp);
		if (!groups[dateKey]) {
			groups[dateKey] = { date: dateKey, items: [] };
		}
		groups[dateKey].items.push(item);
	}

	return Object.values(groups);
}

// Memoized history item component to prevent re-renders when parent updates
interface HistoryItemProps {
	entry: HistoryEntry;
	onCopy: (text: string) => void;
	onDelete: (id: string) => void;
	isDeleting: boolean;
}

const HistoryItem = memo(function HistoryItem({
	entry,
	onCopy,
	onDelete,
	isDeleting,
}: HistoryItemProps) {
	const [isExpanded, setIsExpanded] = useState(false);
	const activeAppContextSnapshot = entry.active_app_context;
	const activeAppContextFields = [
		{
			label: "App",
			value: normalizeMeaningfulContextValue(
				activeAppContextSnapshot?.focused_application?.display_name,
			),
		},
		{
			label: "Window",
			value: normalizeMeaningfulContextValue(
				activeAppContextSnapshot?.focused_window?.title,
			),
		},
		{
			label: "Tab",
			value: normalizeMeaningfulContextValue(
				activeAppContextSnapshot?.focused_browser_tab?.title,
			),
		},
		{
			label: "Origin",
			value: normalizeMeaningfulContextValue(
				activeAppContextSnapshot?.focused_browser_tab?.origin,
			),
		},
	].filter((activeAppContextField) => activeAppContextField.value !== null);

	return (
		<div className="history-item">
			<span className="history-time">{formatTime(entry.timestamp)}</span>
			<div className="history-content">
				<p className="history-text">{entry.text}</p>
				{isExpanded && (
					<div className="history-raw-text">
						<Text className="history-details-heading" mb={4}>
							Raw transcription:
						</Text>
						<Text className="history-details-content">{entry.raw_text}</Text>

						{activeAppContextSnapshot && (
							<>
								<Text className="history-details-heading" mt={12} mb={4}>
									Active app context:
								</Text>
								{activeAppContextFields.length > 0 ? (
									<div className="history-context-grid">
										{activeAppContextFields.map(({ label, value }) => (
											<div key={label} className="history-context-row">
												<Text className="history-context-label">{label}</Text>
												<Text className="history-context-value">{value}</Text>
											</div>
										))}
									</div>
								) : (
									<Text className="history-details-content">
										No active app details were available for this dictation.
									</Text>
								)}
							</>
						)}
					</div>
				)}
			</div>
			<div className="history-actions">
				<Menu shadow="md" width={180} position="bottom-end">
					<Menu.Target>
						<ActionIcon variant="subtle" size="sm" color="gray">
							<MoreVertical size={14} />
						</ActionIcon>
					</Menu.Target>
					<Menu.Dropdown>
						<Menu.Item
							leftSection={<Copy size={14} />}
							onClick={() => onCopy(entry.text)}
						>
							Copy
						</Menu.Item>
						<Menu.Item
							leftSection={<Copy size={14} />}
							onClick={() => onCopy(entry.raw_text)}
						>
							Copy raw
						</Menu.Item>
						<Menu.Item
							leftSection={
								isExpanded ? <EyeOff size={14} /> : <Eye size={14} />
							}
							onClick={() => setIsExpanded((currentState) => !currentState)}
						>
							{isExpanded ? "Hide" : "View"} details
						</Menu.Item>
						<Menu.Divider />
						<Menu.Item
							color="red"
							leftSection={<Trash2 size={14} />}
							onClick={() => onDelete(entry.id)}
							disabled={isDeleting}
						>
							Delete
						</Menu.Item>
					</Menu.Dropdown>
				</Menu>
			</div>
		</div>
	);
});

export function HistoryFeed() {
	const queryClient = useQueryClient();
	const { data: history, isLoading, error } = useHistory(100);
	const deleteEntry = useDeleteHistoryEntry();
	const clearHistory = useClearHistory();
	const clipboard = useClipboard();
	const [confirmOpened, { open: openConfirm, close: closeConfirm }] =
		useDisclosure(false);

	// Listen for history changes from other windows (e.g., overlay after transcription)
	useEffect(() => {
		let unlisten: (() => void) | undefined;

		const setup = async () => {
			unlisten = await tauriAPI.onHistoryChanged(() => {
				queryClient.invalidateQueries({ queryKey: ["history"] });
			});
		};

		setup();

		return () => {
			unlisten?.();
		};
	}, [queryClient]);

	const handleDelete = useCallback(
		(id: string) => {
			deleteEntry.mutate(id);
		},
		[deleteEntry],
	);

	const handleCopy = useCallback(
		(text: string) => {
			clipboard.copy(text);
		},
		[clipboard],
	);

	const handleClearAll = () => {
		clearHistory.mutate(undefined, {
			onSuccess: () => {
				closeConfirm();
			},
		});
	};

	// useMemo must be called unconditionally (before any early returns)
	const groupedHistory = useMemo(
		() => groupHistoryByDate(history ?? []),
		[history],
	);

	if (isLoading) {
		return (
			<div className="animate-in animate-in-delay-2">
				<div className="section-header">
					<span className="section-title">History</span>
				</div>
				<div className="empty-state">
					<p className="empty-state-text">Loading history...</p>
				</div>
			</div>
		);
	}

	if (error) {
		return (
			<div className="animate-in animate-in-delay-2">
				<div className="section-header">
					<span className="section-title">History</span>
				</div>
				<div className="empty-state">
					<p className="empty-state-text" style={{ color: "#ef4444" }}>
						Failed to load history
					</p>
				</div>
			</div>
		);
	}

	if (!history || history.length === 0) {
		return (
			<div className="animate-in animate-in-delay-2">
				<div className="section-header">
					<span className="section-title">History</span>
				</div>
				<div className="empty-state">
					<MessageSquare className="empty-state-icon" />
					<h4 className="empty-state-title">No dictation history yet</h4>
					<p className="empty-state-text">
						Your transcribed text will appear here after you use voice
						dictation.
					</p>
				</div>
			</div>
		);
	}

	return (
		<div className="animate-in animate-in-delay-2">
			<div className="section-header">
				<span className="section-title">History</span>
				<Button
					variant="subtle"
					size="compact-sm"
					color="gray"
					onClick={openConfirm}
					disabled={clearHistory.isPending}
				>
					Clear All
				</Button>
			</div>

			<Modal
				opened={confirmOpened}
				onClose={closeConfirm}
				title="Clear History"
				centered
				size="sm"
			>
				<Text size="sm" mb="lg">
					Are you sure you want to clear all history? This action cannot be
					undone.
				</Text>
				<Group justify="flex-end">
					<Button variant="default" onClick={closeConfirm}>
						Cancel
					</Button>
					<Button
						color="red"
						onClick={handleClearAll}
						loading={clearHistory.isPending}
					>
						Clear All
					</Button>
				</Group>
			</Modal>

			{groupedHistory.map((group) => (
				<div key={group.date} style={{ marginBottom: 24 }}>
					<p
						className="section-title"
						style={{ marginBottom: 12, fontSize: 11 }}
					>
						{group.date}
					</p>
					<div className="history-feed">
						{group.items.map((entry) => (
							<HistoryItem
								key={entry.id}
								entry={entry}
								onCopy={handleCopy}
								onDelete={handleDelete}
								isDeleting={deleteEntry.isPending}
							/>
						))}
					</div>
				</div>
			))}
		</div>
	);
}
