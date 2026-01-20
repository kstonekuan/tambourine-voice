// biome-ignore-all lint/complexity/useLiteralKeys: https://github.com/biomejs/biome/issues/463

import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import svgr from "vite-plugin-svgr";

const host = process.env["TAURI_DEV_HOST"];

export default defineConfig({
	plugins: [react(), tailwindcss(), svgr()],
	clearScreen: false,
	server: {
		host: host || false,
		port: 5173,
		strictPort: true,
		hmr: host
			? {
					protocol: "ws",
					host,
					port: 5173,
				}
			: undefined,
		watch: {
			ignored: ["**/src-tauri/**"],
		},
	},
	envPrefix: ["VITE_", "TAURI_"],
	build: {
		target:
			process.env["TAURI_PLATFORM"] === "windows"
				? "chrome105"
				: process.env["TAURI_PLATFORM"] === "macos"
					? "safari13"
					: "chrome105",
		minify: !process.env["TAURI_DEBUG"] ? "esbuild" : false,
		sourcemap: !!process.env["TAURI_DEBUG"],
		rollupOptions: {
			input: {
				main: "index.html",
				overlay: "overlay.html",
			},
		},
	},
});
