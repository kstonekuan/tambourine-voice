import { MantineProvider } from "@mantine/core";
import "@mantine/core/styles.css";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import OverlayApp from "./OverlayApp";

// Styles are imported in OverlayApp.tsx via app.css

const queryClient = new QueryClient({
	defaultOptions: {
		queries: { retry: 2 },
		mutations: { retry: 1 },
	},
});

const rootElement = document.getElementById("root");
if (!rootElement) {
	throw new Error("Root element not found");
}

createRoot(rootElement).render(
	<StrictMode>
		<QueryClientProvider client={queryClient}>
			<MantineProvider defaultColorScheme="dark">
				<OverlayApp />
			</MantineProvider>
		</QueryClientProvider>
	</StrictMode>,
);
