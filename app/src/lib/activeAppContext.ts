export type FocusEventSource = "polling" | "accessibility" | "uia" | "unknown";
export type FocusConfidenceLevel = "high" | "medium" | "low";

export type FocusedApplication = {
	display_name: string;
	bundle_id?: string | null;
	process_path?: string | null;
};

export type FocusedWindow = {
	title: string;
};

export type FocusedBrowserTab = {
	title?: string | null;
	origin?: string | null;
	browser?: string | null;
};

export type ActiveAppContextSnapshot = {
	focused_application?: FocusedApplication | null;
	focused_window?: FocusedWindow | null;
	focused_browser_tab?: FocusedBrowserTab | null;
	event_source: FocusEventSource;
	confidence_level: FocusConfidenceLevel;
	captured_at: string;
};
