import tailwindcss from "@tailwindcss/vite";

import { tanstackStart } from "@tanstack/react-start/plugin/vite";

import viteReact from "@vitejs/plugin-react";
import { nitro } from "nitro/vite";
import { defineConfig } from "vite";

const config = defineConfig({
	resolve: { tsconfigPaths: true },
	build: {
		chunkSizeWarningLimit: 1000,
	},
	// Mark Shiki + Mermaid as external for the SSR builds — both ship runtime
	// assets (onig.wasm, dynamic d3 graphs) that the bundler cannot follow at
	// build time. The browser-side transcript renderer is the only consumer
	// and it loads them on demand.
	ssr: {
		external: ["shiki", "@shikijs", "mermaid", "@mermaid-js"],
		noExternal: ["@xero/ui"],
	},
	environments: {
		ssr: {
			resolve: {
				external: ["shiki", "@shikijs", "mermaid", "@mermaid-js"],
				noExternal: ["@xero/ui"],
			},
		},
		server: {
			resolve: {
				external: ["shiki", "@shikijs", "mermaid", "@mermaid-js"],
				noExternal: ["@xero/ui"],
			},
		},
	},
	plugins: [
		nitro({
			rollupConfig: {
				external: [
					/^@sentry\//,
					/^shiki(\/|$)/,
					/^@shikijs(\/|$)/,
					/^mermaid(\/|$)/,
					/^@mermaid-js(\/|$)/,
				],
			},
		}),
		tailwindcss(),
		tanstackStart(),
		viteReact(),
	],
});

export default config;
