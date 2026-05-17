const DEFAULT_SERVER_URL = "http://127.0.0.1:4000";
const RUNTIME_CONFIG_KEY = "__XERO_RUNTIME_CONFIG__";

interface XeroRuntimeConfig {
	serverUrl?: string;
}

declare global {
	interface Window {
		[RUNTIME_CONFIG_KEY]?: XeroRuntimeConfig;
	}
}

// Server URL — points at the Phoenix relay/auth API. SSR reads the runtime
// `XERO_SERVER_URL`; the browser reads the value injected by `RootDocument`.
export function getServerUrl(): string {
	return normalizeServerUrl(
		getBrowserRuntimeServerUrl() ??
			getProcessServerUrl() ??
			getViteServerUrl() ??
			DEFAULT_SERVER_URL,
	);
}

export function getPublicRuntimeConfigScript(): string {
	const config: XeroRuntimeConfig = { serverUrl: getServerUrl() };
	const json = JSON.stringify(config).replaceAll("<", "\\u003c");
	return `window.${RUNTIME_CONFIG_KEY}=${json};`;
}

function getBrowserRuntimeServerUrl(): string | undefined {
	if (typeof window === "undefined") return undefined;
	return window[RUNTIME_CONFIG_KEY]?.serverUrl;
}

function getProcessServerUrl(): string | undefined {
	const globalWithProcess = globalThis as {
		process?: { env?: Record<string, string | undefined> };
	};
	return globalWithProcess.process?.env?.XERO_SERVER_URL;
}

function getViteServerUrl(): string | undefined {
	return (import.meta as { env?: { VITE_XERO_SERVER_URL?: string } }).env
		?.VITE_XERO_SERVER_URL;
}

function normalizeServerUrl(value: string): string {
	const trimmed = value.trim();
	return trimmed.endsWith("/") ? trimmed.slice(0, -1) : trimmed;
}
