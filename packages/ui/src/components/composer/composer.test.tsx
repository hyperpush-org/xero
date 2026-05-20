/** @vitest-environment jsdom */

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Composer, type ComposerDictationLike, type ComposerProps } from "./composer";

const inertDictation: ComposerDictationLike = {
	ariaLabel: "Start dictation",
	isListening: false,
	isToggleDisabled: false,
	phase: "idle",
	tooltip: "Dictate",
	toggle: vi.fn(async () => undefined),
	isVisible: false,
};

function setViewportWidth(width: number) {
	Object.defineProperty(window, "innerWidth", {
		configurable: true,
		writable: true,
		value: width,
	});
}

function stubMatchMedia(matches: boolean) {
	Object.defineProperty(window, "matchMedia", {
		configurable: true,
		writable: true,
		value: (query: string) =>
			({
				matches,
				media: query,
				onchange: null,
				addEventListener: () => {},
				removeEventListener: () => {},
				addListener: () => {},
				removeListener: () => {},
				dispatchEvent: () => false,
			}) as MediaQueryList,
	});
}

type RenderOverrides = Partial<Omit<ComposerProps, "draftPrompt" | "onDraftPromptChange">> & {
	initialDraft?: string;
};

function renderComposer({ initialDraft = "", ...overrides }: RenderOverrides = {}) {
	const onSubmit = overrides.onSubmit ?? vi.fn();
	function Harness() {
		const [draft, setDraft] = useState(initialDraft);
		return (
			<Composer
				draftPrompt={draft}
				onDraftPromptChange={setDraft}
				agentGroups={[{ id: "agents", options: [{ id: "ask", label: "Ask" }] }]}
				selectedAgentId="ask"
				onAgentChange={vi.fn()}
				modelGroups={[{ id: "models", options: [{ id: "gpt", label: "GPT" }] }]}
				selectedModelId="gpt"
				onModelChange={vi.fn()}
				dictation={inertDictation}
				{...overrides}
				onSubmit={onSubmit}
			/>
		);
	}
	render(<Harness />);
	return { onSubmit };
}

describe("Composer", () => {
	beforeEach(() => {
		setViewportWidth(1280);
		stubMatchMedia(false);
	});

	afterEach(() => {
		vi.clearAllMocks();
	});

	it("renders inline agent and model selectors and submits typed text", async () => {
		const { onSubmit } = renderComposer();

		expect(screen.getByLabelText("Agent selector")).toBeVisible();
		expect(screen.getByLabelText("Model selector")).toBeVisible();

		const textarea = screen.getByRole("textbox") as HTMLTextAreaElement;
		fireEvent.change(textarea, { target: { value: "Ship it" } });
		fireEvent.click(screen.getByRole("button", { name: "Send message" }));

		await waitFor(() => expect(onSubmit).toHaveBeenCalledWith("Ship it"));
	});

	it("keeps the send button disabled until there is text or an attachment", () => {
		renderComposer();
		expect(
			(screen.getByRole("button", { name: "Send message" }) as HTMLButtonElement)
				.disabled,
		).toBe(true);
	});

	it("shows the approval selector only when more than one option is available", () => {
		const { onSubmit } = renderComposer({
			approvalOptions: [{ id: "suggest", label: "Suggest" }],
			selectedApprovalId: "suggest",
			onApprovalChange: vi.fn(),
		});
		expect(screen.queryByLabelText("Approval mode selector")).toBeNull();
		void onSubmit;
	});

	it("renders the approval selector with multiple options", () => {
		renderComposer({
			approvalOptions: [
				{ id: "suggest", label: "Suggest" },
				{ id: "auto", label: "Auto-edit" },
			],
			selectedApprovalId: "suggest",
			onApprovalChange: vi.fn(),
		});
		expect(screen.getByLabelText("Approval mode selector")).toBeVisible();
	});

	it("shows the attach button only when an add-files handler is provided", () => {
		const { onSubmit } = renderComposer();
		expect(screen.queryByRole("button", { name: "Add files" })).toBeNull();
		void onSubmit;
	});

	it("renders the attach button when onAddFiles is provided", () => {
		renderComposer({ onAddFiles: vi.fn() });
		expect(screen.getByRole("button", { name: "Add files" })).toBeVisible();
	});

	it("renders a stop button that invokes onStop while a run is active", () => {
		const onStop = vi.fn();
		renderComposer({ isStopVisible: true, onStop });
		const stopButton = screen.getByRole("button", { name: "Stop agent run" });
		expect(stopButton).toBeVisible();
		fireEvent.click(stopButton);
		expect(onStop).toHaveBeenCalledTimes(1);
	});

	it("collapses controls behind a settings trigger on mobile viewports", () => {
		setViewportWidth(375);
		stubMatchMedia(true);
		renderComposer();
		expect(screen.getByRole("button", { name: "Composer settings" })).toBeVisible();
		expect(screen.queryByLabelText("Agent selector")).toBeNull();
	});
});
