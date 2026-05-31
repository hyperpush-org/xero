import tailwindcss from "@tailwindcss/vite";

import { tanstackStart } from "@tanstack/react-start/plugin/vite";

import viteReact from "@vitejs/plugin-react";
import { nitro } from "nitro/vite";
import { defineConfig, type Plugin } from "vite";
import { XERO_CLOUD_PWA_ROUTE_RULES } from "./src/lib/pwa/deployment-headers";
import {
	renderXeroCloudServiceWorker,
	XERO_CLOUD_PUBLIC_PRECACHE_URLS,
} from "./src/lib/pwa/service-worker-template";

const config = defineConfig(({ mode }) => {
	const isTest = mode === "test";

	return {
		resolve: { tsconfigPaths: true },
		build: {
			chunkSizeWarningLimit: 1000,
			rolldownOptions: {
				checks: {
					pluginTimings: false,
				},
			},
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
			...(isTest
				? []
				: [
						xeroCloudPwaServiceWorkerDev(),
						nitro({
							routeRules: XERO_CLOUD_PWA_ROUTE_RULES,
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
						tanstackStart(),
						xeroCloudPwaServiceWorker(),
					]),
			tailwindcss(),
			viteReact(),
		],
	};
});

export default config;

// Production emits `sw.js` from the build bundle. The dev server never runs that
// bundle step, so without this the PWA is uninstallable locally (no service
// worker means Chromium never fires `beforeinstallprompt`). Serve a service
// worker that precaches only the public shell — dev has no hashed assets, and
// Vite's module URLs (`/@fs`, `/@vite`, `/src`) are network-only in the SW
// fetch policy, so HMR is unaffected.
function xeroCloudPwaServiceWorkerDev(): Plugin {
	return {
		name: "xero-cloud-pwa-service-worker-dev",
		apply: "serve",
		enforce: "pre",
		configureServer(server) {
			server.middlewares.use((req, res, next) => {
				const path = req.url?.split("?", 1)[0];
				if (path !== "/sw.js") {
					next();
					return;
				}
				res.setHeader("Content-Type", "text/javascript; charset=utf-8");
				res.setHeader("Cache-Control", "no-cache, no-store, must-revalidate");
				res.setHeader("Service-Worker-Allowed", "/");
				res.end(renderXeroCloudServiceWorker(XERO_CLOUD_PUBLIC_PRECACHE_URLS));
			});
		},
	};
}

function xeroCloudPwaServiceWorker(): Plugin {
	return {
		name: "xero-cloud-pwa-service-worker",
		apply: "build",
		generateBundle(_options, bundle) {
			const environmentName = (this as { environment?: { name?: string } })
				.environment?.name;
			if (environmentName && environmentName !== "client") return;

			const bundleUrls = Object.values(bundle)
				.map((entry) => entry.fileName)
				.filter((fileName) =>
					/^assets\/.+\.(?:css|js|mjs|png|svg|webp|jpe?g|woff2?)$/.test(
						fileName,
					),
				)
				.map((fileName) => `/${fileName}`);

			if (bundleUrls.length === 0) return;

			this.emitFile({
				type: "asset",
				fileName: "sw.js",
				source: renderXeroCloudServiceWorker([
					...XERO_CLOUD_PUBLIC_PRECACHE_URLS,
					...bundleUrls,
				]),
			});
		},
	};
}
