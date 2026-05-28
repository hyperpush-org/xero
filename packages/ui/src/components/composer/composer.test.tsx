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

function stubResizeObserver() {
	class MockResizeObserver {
		observe() {}
		unobserve() {}
		disconnect() {}
	}

	Object.defineProperty(window, "ResizeObserver", {
		configurable: true,
		writable: true,
		value: MockResizeObserver,
	});
	Object.defineProperty(globalThis, "ResizeObserver", {
		configurable: true,
		writable: true,
		value: MockResizeObserver,
	});
	Object.defineProperty(Element.prototype, "scrollIntoView", {
		configurable: true,
		writable: true,
		value: vi.fn(),
	});
}

function stubNavigatorPlatform(platform: string) {
	Object.defineProperty(window.navigator, "platform", {
		configurable: true,
		value: platform,
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
		stubNavigatorPlatform("Win32");
		stubMatchMedia(false);
		stubResizeObserver();
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

	it("combines model and thinking into one selector", () => {
		const onThinkingChange = vi.fn();
		renderComposer({
			thinkingOptions: [
				{ id: "low", label: "Low" },
				{ id: "high", label: "High" },
			],
			selectedThinkingId: "low",
			onThinkingChange,
		});

		const selector = screen.getByRole("combobox", {
			name: "Model and thinking selector",
		});
		expect(selector).toHaveTextContent("GPT");
		expect(selector).toHaveTextContent("Low");
		expect(screen.queryByLabelText("Thinking level selector")).toBeNull();

		fireEvent.pointerDown(selector, { button: 0 });
		const thinkingItem = screen.getByRole("menuitem", { name: /Thinking/i });
		fireEvent.keyDown(thinkingItem, { key: "ArrowRight" });
		fireEvent.click(screen.getByRole("menuitemradio", { name: "High" }));

		expect(onThinkingChange).toHaveBeenCalledWith("high");
	});

	it("toggles visible dictation from the composer shortcut and renders the voice meter", async () => {
		const toggle = vi.fn(async () => undefined);
		renderComposer({
			dictation: {
				...inertDictation,
				audioLevel: 0.62,
				ariaLabel: "Stop dictation",
				isListening: true,
				isVisible: true,
				phase: "listening",
				toggle,
			},
		});

		expect(document.querySelector(".composer-dictation-waveform")).not.toBeNull();
		fireEvent.keyDown(window, { key: "d", ctrlKey: true, shiftKey: true });

		await waitFor(() => expect(toggle).toHaveBeenCalledTimes(1));
	});

	it("keeps only the model list scrollable when many models are available", () => {
		renderComposer({
			modelGroups: [
				{
					id: "openai",
					label: "OpenAI",
					options: Array.from({ length: 40 }, (_, index) => ({
						id: `model-${index}`,
						label: `Model ${index}`,
					})),
				},
			],
			selectedModelId: "model-0",
			thinkingOptions: [{ id: "low", label: "Low" }],
			selectedThinkingId: "low",
			onThinkingChange: vi.fn(),
		});

		fireEvent.pointerDown(
			screen.getByRole("combobox", { name: "Model and thinking selector" }),
			{ button: 0 },
		);

		const dropdown = document.querySelector('[data-slot="dropdown-menu-content"]');
		const modelList = document.querySelector('[data-slot="command-list"]');

		expect(dropdown).toHaveClass("overflow-visible");
		expect(dropdown).not.toHaveClass("overflow-y-auto");
		expect(modelList).toHaveClass(
			"max-h-[min(18rem,calc(var(--radix-dropdown-menu-content-available-height)_-_5rem))]",
		);
		expect(modelList).toHaveClass("overflow-y-auto");
	});

	it("keeps the combined selector open after choosing a model so thinking can be adjusted", () => {
		const onModelChange = vi.fn();
		const onThinkingChange = vi.fn();
		renderComposer({
			modelGroups: [
				{
					id: "models",
					options: [
						{ id: "gpt", label: "GPT" },
						{ id: "grok", label: "Grok" },
					],
				},
			],
			selectedModelId: "gpt",
			onModelChange,
			thinkingOptions: [
				{ id: "low", label: "Low" },
				{ id: "high", label: "High" },
			],
			selectedThinkingId: "low",
			onThinkingChange,
		});

		const selector = screen.getByRole("combobox", {
			name: "Model and thinking selector",
		});
		fireEvent.pointerDown(selector, { button: 0 });
		fireEvent.click(screen.getByRole("option", { name: "Grok" }));

		expect(onModelChange).toHaveBeenCalledWith("grok");
		expect(screen.getByRole("menuitem", { name: /Thinking/i })).toBeVisible();

		fireEvent.keyDown(screen.getByRole("menuitem", { name: /Thinking/i }), {
			key: "ArrowRight",
		});
		fireEvent.click(screen.getByRole("menuitemradio", { name: "High" }));

		expect(onThinkingChange).toHaveBeenCalledWith("high");
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
			approvalOptions: [{ id: "suggest", label: "Ask first" }],
			selectedApprovalId: "suggest",
			onApprovalChange: vi.fn(),
		});
		expect(screen.queryByLabelText("Approval mode selector")).toBeNull();
		void onSubmit;
	});

	it("renders the approval selector with multiple options", () => {
		renderComposer({
			approvalOptions: [
				{ id: "suggest", label: "Ask first" },
				{ id: "auto", label: "Auto-edit files" },
			],
			selectedApprovalId: "suggest",
			onApprovalChange: vi.fn(),
		});
		expect(screen.queryByLabelText("Approval mode selector")).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Composer settings" }));
		expect(screen.getByRole("combobox", { name: "Approval mode" })).toBeVisible();
	});

	it("moves auto compact into the desktop composer settings dialog", () => {
		const onAutoCompactEnabledChange = vi.fn();
		renderComposer({
			autoCompactEnabled: true,
			onAutoCompactEnabledChange,
		});

		expect(screen.queryByRole("button", { name: "Auto-compact before sending" })).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Composer settings" }));

		const autoCompactSwitch = screen.getByRole("switch", {
			name: "Auto-compact before sending",
		});
		expect(autoCompactSwitch).toHaveAttribute("aria-checked", "true");
		fireEvent.click(autoCompactSwitch);
		expect(onAutoCompactEnabledChange).toHaveBeenCalledWith(false);
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

	it("keeps model, thinking, and auto compact as separate controls in the mobile drawer", () => {
		setViewportWidth(375);
		stubMatchMedia(true);
		renderComposer({
			thinkingOptions: [
				{ id: "low", label: "Low" },
				{ id: "high", label: "High" },
			],
			selectedThinkingId: "low",
			onThinkingChange: vi.fn(),
			autoCompactEnabled: false,
			onAutoCompactEnabledChange: vi.fn(),
		});

		fireEvent.click(screen.getByRole("button", { name: "Composer settings" }));

		expect(screen.getByRole("combobox", { name: "Agent" })).toBeVisible();
		expect(screen.getByRole("combobox", { name: "Model selector" })).toBeVisible();
		expect(screen.getByRole("combobox", { name: "Thinking" })).toBeVisible();
		expect(screen.getByRole("switch", { name: "Auto-compact before sending" })).toBeVisible();
		expect(screen.queryByRole("combobox", { name: "Model and thinking selector" })).toBeNull();
	});
});
