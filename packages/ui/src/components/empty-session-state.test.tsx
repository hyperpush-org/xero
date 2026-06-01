/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { EmptySessionState } from "./empty-session-state";

describe("EmptySessionState", () => {
	afterEach(() => {
		cleanup();
	});

	it("uses a smaller title for Computer Use empty state", () => {
		render(
			<EmptySessionState
				context="computer-use"
				projectLabel="Computer Use"
			/>,
		);

		const heading = screen.getByRole("heading", { name: "Computer Use" });
		expect(heading).toHaveClass("text-[22px]");
		expect(heading).toHaveClass("sm:text-2xl");
		expect(heading).not.toHaveClass("sm:text-[26px]");
	});
});
