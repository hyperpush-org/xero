import { describe, expect, it } from "vitest";

import { getXeroCloudServiceWorkerPolicy } from "./service-worker-routes";

const ORIGIN = "https://cloud.xeroshell.test";

function policy(
	path: string,
	options: {
		method?: string;
		mode?: RequestMode;
		destination?: RequestDestination;
		credentials?: RequestCredentials;
		headers?: Record<string, string>;
	} = {},
) {
	return getXeroCloudServiceWorkerPolicy(
		{
			url: new URL(path, ORIGIN).toString(),
			...options,
		},
		ORIGIN,
	);
}

describe("getXeroCloudServiceWorkerPolicy", () => {
	it("allows static build and PWA assets to use the static cache", () => {
		expect(policy("/assets/app-Bc123.js")).toBe("static-cache");
		expect(policy("/icons/icon-192x192.png")).toBe("static-cache");
		expect(policy("/manifest.webmanifest")).toBe("static-cache");
		expect(policy("/offline.html")).toBe("static-cache");
	});

	it("uses a network-first offline fallback for top-level navigations only", () => {
		expect(policy("/sessions", { mode: "navigate" })).toBe(
			"navigation-fallback",
		);
		expect(
			policy("/sessions/desktop/session", { destination: "document" }),
		).toBe("navigation-fallback");
	});

	it("keeps TanStack server functions, auth, relay, and socket URLs out of caches", () => {
		expect(policy("/_serverFn/getCurrentSession")).toBe("network-only");
		expect(policy("/api/github/session")).toBe("network-only");
		expect(policy("/api/relay/token/refresh")).toBe("network-only");
		expect(policy("/socket/web")).toBe("network-only");
		expect(policy("/oauth/github/callback")).toBe("network-only");
	});

	it("keeps credentialed, authorized, non-GET, and cross-origin requests network-only", () => {
		expect(policy("/assets/app-Bc123.js", { credentials: "include" })).toBe(
			"network-only",
		);
		expect(
			policy("/assets/app-Bc123.js", {
				headers: { authorization: "Bearer relay-token" },
			}),
		).toBe("network-only");
		expect(policy("/assets/app-Bc123.js", { method: "POST" })).toBe(
			"network-only",
		);
		expect(
			getXeroCloudServiceWorkerPolicy(
				{ url: "https://api.xeroshell.test/assets/app.js" },
				ORIGIN,
			),
		).toBe("network-only");
	});
});
