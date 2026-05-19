const DEFAULT_SERVER_URL = "http://127.0.0.1:4000";
const RUNTIME_CONFIG_KEY = "__XERO_RUNTIME_CONFIG__";
export const RUNTIME_SERVER_URL_META_NAME = "xero-server-url";

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

export function getPublicRuntimeServerUrl(): string {
	return getServerUrl();
}

export function getCanonicalLoopbackCloudUrl(
	currentHref?: string,
	serverUrl = getServerUrl(),
): string | null {
	const href = currentHref ?? getCurrentHref();
	if (!href) return null;

	try {
		const current = new URL(href);
		const server = new URL(serverUrl);
		if (
			!isLoopbackHostname(current.hostname) ||
			!isLoopbackHostname(server.hostname) ||
			sameHostname(current.hostname, server.hostname)
		) {
			return null;
		}

		current.hostname = server.hostname;
		return current.toString();
	} catch {
		return null;
	}
}

function getCurrentHref(): string | undefined {
	if (typeof window !== "undefined") return window.location.href;
	return undefined;
}

function getBrowserRuntimeServerUrl(): string | undefined {
	if (typeof window === "undefined") return undefined;
	return (
		window[RUNTIME_CONFIG_KEY]?.serverUrl ??
		window.document
			?.querySelector(`meta[name="${RUNTIME_SERVER_URL_META_NAME}"]`)
			?.getAttribute("content") ??
		undefined
	);
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

function isLoopbackHostname(hostname: string): boolean {
	return (
		normalizeHostname(hostname) === "localhost" ||
		normalizeHostname(hostname) === "127.0.0.1" ||
		normalizeHostname(hostname) === "::1"
	);
}

function sameHostname(left: string, right: string): boolean {
	return normalizeHostname(left) === normalizeHostname(right);
}

function normalizeHostname(hostname: string): string {
	return hostname.toLowerCase().replace(/^\[/, "").replace(/\]$/, "");
}
