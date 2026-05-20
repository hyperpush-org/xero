import { describe, expect, it } from "vitest";

import { getRouter } from "./router";

describe("cloud router", () => {
	it("does not rely on a timing delay to hide session navigation pending UI", () => {
		const router = getRouter();

		expect(router.options.defaultPendingMs).toBe(0);
		expect(router.options.defaultPendingMinMs).toBe(0);
		expect(router.options.defaultPendingComponent).toBeTypeOf("function");
	});
});
