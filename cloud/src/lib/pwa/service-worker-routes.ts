export type XeroCloudServiceWorkerPolicy =
	| "network-only"
	| "navigation-fallback"
	| "static-cache";

export const XERO_CLOUD_SENSITIVE_PATH_PREFIXES = [
	"/_serverFn/",
	"/api/",
	"/auth/",
	"/github/",
	"/oauth/",
	"/relay/",
	"/socket/",
] as const;

export const XERO_CLOUD_CACHEABLE_STATIC_PATH_PREFIXES = [
	"/assets/",
	"/icons/",
] as const;

export const XERO_CLOUD_CACHEABLE_STATIC_EXACT_PATHS = [
	"/apple-touch-icon.png",
	"/manifest.webmanifest",
	"/offline.html",
] as const;

const SENSITIVE_HEADER_NAMES = [
	"authorization",
	"cookie",
	"x-tss-serialized",
	"x-tsr-serverfn",
	"x-xero-github-session-id",
] as const;

interface HeaderReader {
	get(name: string): string | null | undefined;
}

export interface XeroCloudServiceWorkerPolicyRequest {
	url: string;
	method?: string;
	mode?: RequestMode | string;
	destination?: RequestDestination | string;
	credentials?: RequestCredentials | string;
	headers?: HeaderReader | Record<string, string | undefined>;
}

export function getXeroCloudServiceWorkerPolicy(
	request: XeroCloudServiceWorkerPolicyRequest,
	scopeOrigin = "https://cloud.xeroshell.test",
): XeroCloudServiceWorkerPolicy {
	if ((request.method ?? "GET").toUpperCase() !== "GET") {
		return "network-only";
	}

	const url = parseRequestUrl(request.url, scopeOrigin);
	if (!url) return "network-only";

	const origin = parseScopeOrigin(scopeOrigin);
	if (!origin || url.origin !== origin) return "network-only";

	if (isSensitivePath(url.pathname)) return "network-only";

	if (isNavigationRequest(request)) return "navigation-fallback";

	if (hasSensitiveHeaders(request.headers)) return "network-only";
	if (request.credentials === "include") return "network-only";

	if (isCacheableStaticPath(url.pathname)) return "static-cache";

	return "network-only";
}

export function isSensitivePath(pathname: string): boolean {
	return XERO_CLOUD_SENSITIVE_PATH_PREFIXES.some((prefix) => {
		const exact = prefix.endsWith("/") ? prefix.slice(0, -1) : prefix;
		return pathname === exact || pathname.startsWith(prefix);
	});
}

export function isCacheableStaticPath(pathname: string): boolean {
	return (
		XERO_CLOUD_CACHEABLE_STATIC_EXACT_PATHS.includes(
			pathname as (typeof XERO_CLOUD_CACHEABLE_STATIC_EXACT_PATHS)[number],
		) ||
		XERO_CLOUD_CACHEABLE_STATIC_PATH_PREFIXES.some((prefix) =>
			pathname.startsWith(prefix),
		)
	);
}

function isNavigationRequest(
	request: XeroCloudServiceWorkerPolicyRequest,
): boolean {
	return request.mode === "navigate" || request.destination === "document";
}

function parseRequestUrl(url: string, scopeOrigin: string): URL | undefined {
	try {
		return new URL(url, scopeOrigin);
	} catch {
		return undefined;
	}
}

function parseScopeOrigin(scopeOrigin: string): string | undefined {
	try {
		return new URL(scopeOrigin).origin;
	} catch {
		return undefined;
	}
}

function hasSensitiveHeaders(
	headers: XeroCloudServiceWorkerPolicyRequest["headers"],
): boolean {
	if (!headers) return false;
	return SENSITIVE_HEADER_NAMES.some((name) =>
		Boolean(readHeader(headers, name)),
	);
}

function readHeader(
	headers: NonNullable<XeroCloudServiceWorkerPolicyRequest["headers"]>,
	name: string,
): string | undefined {
	if ("get" in headers && typeof headers.get === "function") {
		return headers.get(name) ?? undefined;
	}
	const entries = headers as Record<string, string | undefined>;
	return entries[name] ?? entries[name.toLowerCase()];
}
