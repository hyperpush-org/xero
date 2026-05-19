import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { XERO_CLOUD_PWA_ROUTE_RULES } from "./deployment-headers";

describe("XERO_CLOUD_PWA_ROUTE_RULES", () => {
	it("serves the manifest with an installable manifest content type", () => {
		expect(
			XERO_CLOUD_PWA_ROUTE_RULES["/manifest.webmanifest"].headers[
				"content-type"
			],
		).toContain("application/manifest+json");
		expect(
			XERO_CLOUD_PWA_ROUTE_RULES["/manifest.webmanifest"].headers[
				"cache-control"
			],
		).toBe("public, max-age=3600, must-revalidate");
	});

	it("keeps the service worker on a no-cache policy so updates are discovered", () => {
		expect(XERO_CLOUD_PWA_ROUTE_RULES["/sw.js"].headers).toMatchObject({
			"content-type": "text/javascript; charset=utf-8",
			"cache-control": "no-cache, no-store, must-revalidate",
		});
	});

	it("allows long-lived immutable caching for app icons", () => {
		expect(
			XERO_CLOUD_PWA_ROUTE_RULES["/icons/**"].headers["cache-control"],
		).toBe("public, max-age=31536000, immutable");
		expect(
			XERO_CLOUD_PWA_ROUTE_RULES["/apple-touch-icon.png"].headers[
				"cache-control"
			],
		).toBe("public, max-age=31536000, immutable");
	});

	it("keeps the Fly deployment on HTTPS for installability", () => {
		const flyToml = readFileSync(
			new URL("../../../fly.toml", import.meta.url),
			"utf8",
		);
		expect(flyToml).toContain("force_https = true");
	});
});
