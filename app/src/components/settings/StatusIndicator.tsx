import { Box, Loader } from "@mantine/core";
import { Check, X } from "lucide-react";
import type { ComponentType } from "react";
import { match } from "ts-pattern";

export type MutationStatus = "idle" | "pending" | "success" | "error";

type IconConfig = {
	Icon: ComponentType<{ size?: number | string; color?: string }>;
	props: { size?: number | string; color?: string };
};

export function StatusIndicator({ status }: { status: MutationStatus }) {
	const config = match(status)
		.with("idle", () => null)
		.with(
			"pending",
			() => ({ Icon: Loader, props: { size: "xs" } }) as IconConfig,
		)
		.with("success", () => ({
			Icon: Check,
			props: { size: 16, color: "var(--mantine-color-green-6)" },
		}))
		.with("error", () => ({
			Icon: X,
			props: { size: 16, color: "var(--mantine-color-red-6)" },
		}))
		.exhaustive();

	if (!config) return null;

	const { Icon, props } = config;
	return (
		<Box style={{ display: "inline-flex", alignItems: "center" }}>
			<Icon {...props} />
		</Box>
	);
}
