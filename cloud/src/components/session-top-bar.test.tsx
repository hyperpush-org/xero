/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { SessionTopBar } from "./session-top-bar";

describe("SessionTopBar", () => {
	afterEach(() => {
		cleanup();
	});

	it("keeps controls below the standalone safe area", () => {
		const { container } = render(<SessionTopBar title="Mobile PWA session" />);

		const header = container.querySelector("header");
		expect(header?.className).toContain(
			"pt-[max(env(safe-area-inset-top),0.5rem)]",
		);
		expect(
			screen.getByRole("button", { name: "Open sessions list" }),
		).toBeTruthy();
	});

	it("renders Computer Use without a header badge", () => {
		render(<SessionTopBar title="Computer Use" />);

		expect(screen.getByText("Computer Use")).toBeTruthy();
		expect(screen.queryByText(/^Computer$/i)).toBeNull();
	});
});
