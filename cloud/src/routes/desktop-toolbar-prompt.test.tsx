/** @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DesktopToolbarPromptForm } from "./sessions.$computerId.$sessionId";

describe("DesktopToolbarPromptForm", () => {
	afterEach(() => {
		cleanup();
		vi.clearAllMocks();
	});

	it("submits a trimmed mobile Computer Use prompt and clears the field", () => {
		const onSubmit = vi.fn();
		const onPromptAccepted = vi.fn();
		render(
			<DesktopToolbarPromptForm
				canSend
				isAgentWorking={false}
				onPromptAccepted={onPromptAccepted}
				onSubmit={onSubmit}
				rotated={false}
			/>,
		);

		const input = screen.getByRole("textbox", {
			name: "Tell Computer Use what to do",
		}) as HTMLInputElement;
		fireEvent.change(input, {
			target: { value: "  Open Safari and summarize it  " },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "Send Computer Use prompt" }),
		);

		expect(onSubmit).toHaveBeenCalledWith("Open Safari and summarize it");
		expect(onPromptAccepted).toHaveBeenCalledOnce();
		expect(input.value).toBe("");
	});

	it("keeps the send action disabled while the model is working", () => {
		const onSubmit = vi.fn();
		render(
			<DesktopToolbarPromptForm
				canSend
				isAgentWorking
				onSubmit={onSubmit}
				rotated
			/>,
		);

		const input = screen.getByRole("textbox", {
			name: "Tell Computer Use what to do",
		});
		fireEvent.change(input, { target: { value: "Keep going" } });
		const send = screen.getByRole("button", {
			name: "Send Computer Use prompt",
		}) as HTMLButtonElement;

		expect(send.disabled).toBe(true);
		expect(onSubmit).not.toHaveBeenCalled();
		const form = screen.getByRole("form", { name: "Computer Use prompt" });
		expect(form.className).toContain("desktop-control-mobile-prompt-rotated");
		expect(form.parentElement?.className).toContain(
			"desktop-control-mobile-prompt-slot",
		);
	});
});
