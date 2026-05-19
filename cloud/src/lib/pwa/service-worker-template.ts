import {
	XERO_CLOUD_CACHEABLE_STATIC_EXACT_PATHS,
	XERO_CLOUD_CACHEABLE_STATIC_PATH_PREFIXES,
	XERO_CLOUD_SENSITIVE_PATH_PREFIXES,
} from "./service-worker-routes";

export const XERO_CLOUD_PUBLIC_PRECACHE_URLS = [
	"/apple-touch-icon.png",
	"/icons/favicon-16x16.png",
	"/icons/favicon-32x32.png",
	"/icons/favicon-48x48.png",
	"/icons/icon-192x192.png",
	"/icons/icon-512x512.png",
	"/icons/maskable-icon-192x192.png",
	"/icons/maskable-icon-512x512.png",
	"/manifest.webmanifest",
	"/offline.html",
] as const;

export function renderXeroCloudServiceWorker(precacheUrls: readonly string[]) {
	const urls = [...new Set(precacheUrls)].sort();
	const version = hashUrls(urls);

	return `/* Xero Cloud generated service worker. Do not edit in build output. */
const XERO_CLOUD_VERSION = ${JSON.stringify(version)};
const XERO_CLOUD_CACHE_PREFIX = "xero-cloud-";
const XERO_CLOUD_PRECACHE = \`\${XERO_CLOUD_CACHE_PREFIX}precache-\${XERO_CLOUD_VERSION}\`;
const XERO_CLOUD_OFFLINE_FALLBACK_URL = "/offline.html";
const XERO_CLOUD_PRECACHE_URLS = ${JSON.stringify(urls, null, "\t")};
const XERO_CLOUD_SENSITIVE_PATH_PREFIXES = ${JSON.stringify(XERO_CLOUD_SENSITIVE_PATH_PREFIXES, null, "\t")};
const XERO_CLOUD_CACHEABLE_STATIC_PATH_PREFIXES = ${JSON.stringify(XERO_CLOUD_CACHEABLE_STATIC_PATH_PREFIXES, null, "\t")};
const XERO_CLOUD_CACHEABLE_STATIC_EXACT_PATHS = ${JSON.stringify(XERO_CLOUD_CACHEABLE_STATIC_EXACT_PATHS, null, "\t")};
const XERO_CLOUD_SENSITIVE_HEADER_NAMES = ${JSON.stringify(["authorization", "cookie", "x-tss-serialized", "x-tsr-serverfn", "x-xero-github-session-id"], null, "\t")};

self.addEventListener("install", (event) => {
\tevent.waitUntil(precacheXeroCloudAssets());
});

self.addEventListener("activate", (event) => {
\tevent.waitUntil(cleanupXeroCloudCaches().then(() => self.clients.claim()));
});

self.addEventListener("message", (event) => {
\tif (event.data && event.data.type === "SKIP_WAITING") {
\t\tself.skipWaiting();
\t}
});

self.addEventListener("fetch", (event) => {
\tconst policy = getXeroCloudServiceWorkerPolicy(event.request);
\tif (policy === "navigation-fallback") {
\t\tevent.respondWith(networkFirstNavigation(event.request));
\t\treturn;
\t}
\tif (policy === "static-cache") {
\t\tevent.respondWith(cacheFirstStaticAsset(event.request));
\t}
});

async function precacheXeroCloudAssets() {
\tconst cache = await caches.open(XERO_CLOUD_PRECACHE);
\tawait Promise.all(
\t\tXERO_CLOUD_PRECACHE_URLS.map(async (url) => {
\t\t\ttry {
\t\t\t\tconst response = await fetch(new Request(url, { cache: "reload", credentials: "omit" }));
\t\t\t\tif (response.ok) await cache.put(url, response);
\t\t\t} catch {
\t\t\t\t// A missing optional asset should not block the app shell from installing.
\t\t\t}
\t\t}),
\t);
}

async function cleanupXeroCloudCaches() {
\tconst keys = await caches.keys();
\tawait Promise.all(
\t\tkeys
\t\t\t.filter((key) => key.startsWith(XERO_CLOUD_CACHE_PREFIX) && key !== XERO_CLOUD_PRECACHE)
\t\t\t.map((key) => caches.delete(key)),
\t);
}

async function networkFirstNavigation(request) {
\ttry {
\t\treturn await fetch(request);
\t} catch {
\t\treturn (
\t\t\t(await caches.match(XERO_CLOUD_OFFLINE_FALLBACK_URL, {
\t\t\t\tcacheName: XERO_CLOUD_PRECACHE,
\t\t\t})) ||
\t\t\tnew Response("Xero Cloud is offline.", {
\t\t\t\tstatus: 503,
\t\t\t\theaders: { "content-type": "text/plain;charset=utf-8" },
\t\t\t})
\t\t);
\t}
}

async function cacheFirstStaticAsset(request) {
\tconst cached = await caches.match(request, { cacheName: XERO_CLOUD_PRECACHE });
\tif (cached) return cached;

\tconst response = await fetch(request);
\tif (response.ok && response.type !== "opaque") {
\t\tconst cache = await caches.open(XERO_CLOUD_PRECACHE);
\t\tawait cache.put(request, response.clone());
\t}
\treturn response;
}

function getXeroCloudServiceWorkerPolicy(request) {
\tif ((request.method || "GET").toUpperCase() !== "GET") return "network-only";

\tconst url = new URL(request.url);
\tif (url.origin !== self.location.origin) return "network-only";
\tif (isSensitivePath(url.pathname)) return "network-only";
\tif (request.mode === "navigate" || request.destination === "document") {
\t\treturn "navigation-fallback";
\t}
\tif (hasSensitiveHeaders(request.headers)) return "network-only";
\tif (request.credentials === "include") return "network-only";
\tif (isCacheableStaticPath(url.pathname)) return "static-cache";
\treturn "network-only";
}

function isSensitivePath(pathname) {
\treturn XERO_CLOUD_SENSITIVE_PATH_PREFIXES.some((prefix) => {
\t\tconst exact = prefix.endsWith("/") ? prefix.slice(0, -1) : prefix;
\t\treturn pathname === exact || pathname.startsWith(prefix);
\t});
}

function isCacheableStaticPath(pathname) {
\treturn (
\t\tXERO_CLOUD_CACHEABLE_STATIC_EXACT_PATHS.includes(pathname) ||
\t\tXERO_CLOUD_CACHEABLE_STATIC_PATH_PREFIXES.some((prefix) => pathname.startsWith(prefix))
\t);
}

function hasSensitiveHeaders(headers) {
\treturn XERO_CLOUD_SENSITIVE_HEADER_NAMES.some((name) => Boolean(headers.get(name)));
}
`;
}

function hashUrls(urls: readonly string[]) {
	let hash = 5381;
	for (const char of urls.join("\n")) {
		hash = (hash * 33) ^ char.charCodeAt(0);
	}
	return (hash >>> 0).toString(36);
}
