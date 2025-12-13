import { createTheme, MantineProvider } from "@mantine/core";
import "@mantine/core/styles.css";
import { Notifications } from "@mantine/notifications";
import "@mantine/notifications/styles.css";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";

const queryClient = new QueryClient();

const darkTheme = createTheme({
	primaryColor: "gray",
	fontFamily: "'DM Sans', -apple-system, BlinkMacSystemFont, sans-serif",
	headings: {
		fontFamily: "'Instrument Serif', serif",
		fontWeight: "400",
	},
	colors: {
		dark: [
			"#C1C2C5",
			"#A6A7AB",
			"#909296",
			"#5C5F66",
			"#373A40",
			"#2C2E33",
			"#1A1A1A",
			"#111111",
			"#0A0A0A",
			"#000000",
		],
	},
	components: {
		Paper: {
			defaultProps: {
				bg: "#111111",
			},
		},
		Card: {
			defaultProps: {
				bg: "#111111",
			},
		},
	},
});

const rootElement = document.getElementById("root");
if (!rootElement) {
	throw new Error("Root element not found");
}

createRoot(rootElement).render(
	<StrictMode>
		<QueryClientProvider client={queryClient}>
			<MantineProvider theme={darkTheme} defaultColorScheme="dark">
				<Notifications position="top-right" />
				<App />
			</MantineProvider>
		</QueryClientProvider>
	</StrictMode>,
);
