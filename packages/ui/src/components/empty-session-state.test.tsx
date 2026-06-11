/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { EmptySessionState } from "./empty-session-state";

describe("EmptySessionState", () => {
	afterEach(() => {
		cleanup();
	});

	it("renders the default project prompt with the project name on its own line", () => {
		render(<EmptySessionState projectLabel="tokenloom-stream-repo" />);

		const heading = screen.getByRole("heading", {
			name: "What can we build together in tokenloom-stream-repo?",
		});
		const [questionLine, projectLine] = Array.from(heading.children);

		expect(heading).toHaveAttribute(
			"aria-label",
			"What can we build together in tokenloom-stream-repo?",
		);
		expect(questionLine).toHaveTextContent("What can we build together in");
		expect(questionLine).toHaveClass("block");
		expect(projectLine).toHaveTextContent("tokenloom-stream-repo?");
		expect(projectLine).toHaveClass("block");
		expect(
			screen.queryByText(
				"Just ask — I can read your code, suggest changes, or run a task for you. Everything we do will show up right here.",
			),
		).not.toBeInTheDocument();
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
