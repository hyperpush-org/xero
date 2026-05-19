import { afterEach, describe, expect, it, vi } from "vitest";

import { getDefaultOAuthReturnUrl } from "./oauth";

describe("getDefaultOAuthReturnUrl", () => {
	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it("keeps localhost and 127.0.0.1 aligned so dev cookies survive the OAuth redirect", () => {
		vi.stubGlobal("window", {
			location: new URL("http://localhost:3002/?from=login#top"),
		});

		expect(getDefaultOAuthReturnUrl("http://127.0.0.1:4000")).toBe(
			"http://127.0.0.1:3002/sessions",
		);
	});

	it("keeps the cloud host when the auth server can share the cookie domain", () => {
		vi.stubGlobal("window", {
			location: new URL("https://cloud.xeroshell.com/?source=pwa"),
		});

		expect(getDefaultOAuthReturnUrl("https://xeroshell.com")).toBe(
			"https://cloud.xeroshell.com/sessions",
		);
	});

	it("does not rewrite production installed-app launches to the auth origin", () => {
		vi.stubGlobal("window", {
			location: new URL("https://cloud.xeroshell.com/sessions"),
		});

		expect(getDefaultOAuthReturnUrl("https://xeroshell.com")).toBe(
			"https://cloud.xeroshell.com/sessions",
		);
	});
});
