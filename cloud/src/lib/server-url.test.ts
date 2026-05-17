import { afterEach, describe, expect, it, vi } from "vitest";

import { getPublicRuntimeConfigScript, getServerUrl } from "./server-url";

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

	it("injects the runtime server URL without raw html delimiters", () => {
		process.env.XERO_SERVER_URL = "https://api.example/<tenant>/";

		expect(getPublicRuntimeConfigScript()).toBe(
			'window.__XERO_RUNTIME_CONFIG__={"serverUrl":"https://api.example/\\u003ctenant>"};',
		);
	});
});
