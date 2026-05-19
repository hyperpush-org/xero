import { afterEach, describe, expect, it, vi } from "vitest";

import {
	getCanonicalLoopbackCloudUrl,
	getPublicRuntimeServerUrl,
	getServerUrl,
	RUNTIME_SERVER_URL_META_NAME,
} from "./server-url";

describe("getServerUrl", () => {
	afterEach(() => {
		vi.unstubAllGlobals();
		delete process.env.XERO_SERVER_URL;
	});

	it("uses the browser runtime config before process env", () => {
		process.env.XERO_SERVER_URL = "https://api-from-process.example";
		vi.stubGlobal("window", {
			__XERO_RUNTIME_CONFIG__: {
				serverUrl: "https://api-from-runtime.example/",
			},
		});

		expect(getServerUrl()).toBe("https://api-from-runtime.example");
	});

	it("uses the browser runtime meta tag before process env", () => {
		process.env.XERO_SERVER_URL = "https://api-from-process.example";
		vi.stubGlobal("window", {
			document: {
				querySelector: vi.fn((selector: string) =>
					selector === `meta[name="${RUNTIME_SERVER_URL_META_NAME}"]`
						? {
								getAttribute: () => "https://api-from-meta.example/",
							}
						: null,
				),
			},
		});

		expect(getServerUrl()).toBe("https://api-from-meta.example");
	});

	it("exposes the normalized server URL for the root document meta tag", () => {
		process.env.XERO_SERVER_URL = "https://api.example/<tenant>/";

		expect(getPublicRuntimeServerUrl()).toBe("https://api.example/<tenant>");
	});

	it("canonicalizes local cloud URLs to the server loopback host", () => {
		expect(
			getCanonicalLoopbackCloudUrl(
				"http://localhost:3002/sessions?tab=active#top",
				"http://127.0.0.1:4000",
			),
		).toBe("http://127.0.0.1:3002/sessions?tab=active#top");
	});

	it("does not canonicalize non-loopback cloud URLs", () => {
		expect(
			getCanonicalLoopbackCloudUrl(
				"https://cloud.xeroshell.com/?source=pwa",
				"https://xeroshell.com",
			),
		).toBeNull();
	});

	it("does not leak local loopback canonicalization into production install URLs", () => {
		expect(
			getCanonicalLoopbackCloudUrl(
				"https://cloud.xeroshell.com/sessions/desktop-1/session-1",
				"http://127.0.0.1:4000",
			),
		).toBeNull();
	});
});
