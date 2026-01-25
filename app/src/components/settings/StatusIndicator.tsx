import { Box, Loader } from "@mantine/core";
import { Check, X } from "lucide-react";
import { match } from "ts-pattern";

export type MutationStatus = "idle" | "pending" | "success" | "error";

export function StatusIndicator({ status }: { status: MutationStatus }) {
	const wrapper = (icon: React.ReactNode) => (
		<Box style={{ display: "inline-flex", alignItems: "center" }}>{icon}</Box>
	);

	return match(status)
		.with("idle", () => null)
		.with("pending", () => wrapper(<Loader size="xs" />))
		.with("success", () =>
			wrapper(<Check size={16} color="var(--mantine-color-green-6)" />),
		)
		.with("error", () =>
			wrapper(<X size={16} color="var(--mantine-color-red-6)" />),
		)
		.exhaustive();
}
