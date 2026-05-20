/** @vitest-environment jsdom */

import {
	act,
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import {
	Composer,
	WebComposerContextIndicator,
} from "@xero/ui/components/composer";
import { type ReactNode, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

interface MockSpeechRecognitionResultEntry {
	transcript: string;
	isFinal?: boolean;
}

class MockSpeechRecognition {
	static instances: MockSpeechRecognition[] = [];

	continuous = false;
	interimResults = false;
	lang = "";
	onend: ((event: Event) => void) | null = null;
	onerror:
		| ((event: Event & { error?: string; message?: string }) => void)
		| null = null;
	onresult: ((event: Event & { results: unknown }) => void) | null = null;
	onstart: ((event: Event) => void) | null = null;
	start = vi.fn(() => {
		this.onstart?.(new Event("start"));
	});
	stop = vi.fn(() => {
		this.onend?.(new Event("end"));
	});
	abort = vi.fn();

	constructor() {
		MockSpeechRecognition.instances.push(this);
	}

	emitResult(entries: MockSpeechRecognitionResultEntry[]) {
		const results = entries.map((entry) => {
			const result = [{ transcript: entry.transcript }] as Array<{
				transcript: string;
			}> & {
				isFinal: boolean;
				item: (index: number) => { transcript: string } | undefined;
			};
			result.isFinal = Boolean(entry.isFinal);
			result.item = (index: number) => result[index];
			return result;
		}) as Array<
			Array<{ transcript: string }> & {
				isFinal: boolean;
				item: (index: number) => { transcript: string } | undefined;
			}
		> & {
			item: (index: number) =>
				| (Array<{ transcript: string }> & {
						isFinal: boolean;
						item: (index: number) => { transcript: string } | undefined;
				  })
				| undefined;
		};
		results.item = (index: number) => results[index];
		this.onresult?.(Object.assign(new Event("result"), { results }));
	}
}

function renderComposer({
	initialDraft = "",
	contextMeter = null,
	onSubmit = vi.fn(),
	autoCompactEnabled,
	onAutoCompactEnabledChange,
}: {
	initialDraft?: string;
	contextMeter?: ReactNode;
	onSubmit?: (draftPrompt?: string) => void;
	autoCompactEnabled?: boolean;
	onAutoCompactEnabledChange?: (next: boolean) => void;
} = {}) {
	function Harness() {
		const [draftPrompt, setDraftPrompt] = useState(initialDraft);
		return (
			<Composer
				draftPrompt={draftPrompt}
				onDraftPromptChange={setDraftPrompt}
				onSubmit={onSubmit}
				agentGroups={[{ id: "agents", options: [{ id: "ask", label: "Ask" }] }]}
				selectedAgentId="ask"
				onAgentChange={vi.fn()}
				modelGroups={[
					{ id: "models", options: [{ id: "gpt-5.5", label: "gpt-5.5" }] },
				]}
				selectedModelId="gpt-5.5"
				onModelChange={vi.fn()}
				autoCompactEnabled={autoCompactEnabled}
				onAutoCompactEnabledChange={onAutoCompactEnabledChange}
				contextMeter={contextMeter}
			/>
		);
	}

	render(<Harness />);
	return { onSubmit };
}

function mockComposerTextareaMetrics() {
	const originalGetComputedStyle = window.getComputedStyle.bind(window);
	const originalScrollHeight = Object.getOwnPropertyDescriptor(
		HTMLTextAreaElement.prototype,
		"scrollHeight",
	);

	vi.spyOn(window, "getComputedStyle").mockImplementation((element) => {
		const style = originalGetComputedStyle(element);
		return new Proxy(style, {
			get(target, property, receiver) {
				if (property === "fontSize") return "15px";
				if (property === "lineHeight") return "24px";
				if (property === "paddingTop") return "4px";
				if (property === "paddingBottom") return "4px";
				return Reflect.get(target, property, receiver);
			},
		});
	});

	Object.defineProperty(HTMLTextAreaElement.prototype, "scrollHeight", {
		configurable: true,
		get(this: HTMLTextAreaElement) {
			const rowCount = Math.max(1, this.value.split("\n").length);
			return rowCount * 24 + 8;
		},
	});

	return () => {
		vi.restoreAllMocks();
		if (originalScrollHeight) {
			Object.defineProperty(
				HTMLTextAreaElement.prototype,
				"scrollHeight",
				originalScrollHeight,
			);
		} else {
			Reflect.deleteProperty(HTMLTextAreaElement.prototype, "scrollHeight");
		}
	};
}

function stubMatchMedia() {
	Object.defineProperty(window, "matchMedia", {
		configurable: true,
		writable: true,
		value: (query: string) =>
			({
				matches: false,
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

describe("Composer dictation", () => {
	beforeEach(() => {
		MockSpeechRecognition.instances = [];
		stubMatchMedia();
		Object.defineProperty(window, "webkitSpeechRecognition", {
			configurable: true,
			writable: true,
			value: MockSpeechRecognition,
		});
		Object.defineProperty(window, "requestAnimationFrame", {
			configurable: true,
			writable: true,
			value: (callback: FrameRequestCallback) => {
				callback(0);
				return 0;
			},
		});
	});

	afterEach(() => {
		cleanup();
		Reflect.deleteProperty(window, "webkitSpeechRecognition");
		MockSpeechRecognition.instances = [];
	});

	it("starts browser dictation and streams recognized speech into the composer", async () => {
		renderComposer();

		const micButton = await screen.findByRole("button", {
			name: "Start dictation",
		});
		await waitFor(() =>
			expect((micButton as HTMLButtonElement).disabled).toBe(false),
		);

		fireEvent.click(micButton);
		const recognition = MockSpeechRecognition.instances[0];
		expect(recognition.start).toHaveBeenCalledTimes(1);

		act(() => {
			recognition.emitResult([{ transcript: "Review the logs" }]);
		});

		const textarea = screen.getByRole("textbox") as HTMLTextAreaElement;
		expect(textarea.value).toBe("Review the logs");
		expect(
			screen
				.getByRole("button", { name: "Stop dictation" })
				.getAttribute("aria-pressed"),
		).toBe("true");
	});

	it("stops active dictation before submitting the dictated prompt", async () => {
		const { onSubmit } = renderComposer();

		const micButton = await screen.findByRole("button", {
			name: "Start dictation",
		});
		await waitFor(() =>
			expect((micButton as HTMLButtonElement).disabled).toBe(false),
		);

		fireEvent.click(micButton);
		const recognition = MockSpeechRecognition.instances[0];
		act(() => {
			recognition.emitResult([{ transcript: "Ship the fix", isFinal: true }]);
		});

		await waitFor(() => {
			expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe(
				"Ship the fix",
			);
		});
		fireEvent.click(screen.getByRole("button", { name: "Send message" }));

		await waitFor(() => expect(recognition.stop).toHaveBeenCalledTimes(1));
		await waitFor(() => expect(onSubmit).toHaveBeenCalledWith("Ship the fix"));
	});
});

describe("Composer layout", () => {
	let restoreTextareaMetrics: (() => void) | null = null;

	beforeEach(() => {
		stubMatchMedia();
	});

	afterEach(() => {
		cleanup();
		restoreTextareaMetrics?.();
		restoreTextareaMetrics = null;
	});

	it("keeps textarea overflow hidden before the six-row cap", async () => {
		restoreTextareaMetrics = mockComposerTextareaMetrics();
		renderComposer({ initialDraft: "One tidy line" });

		const textarea = screen.getByRole("textbox") as HTMLTextAreaElement;
		await waitFor(() => {
			expect(textarea.style.height).toBe("32px");
			expect(textarea.style.overflowY).toBe("hidden");
		});
	});

	it("enables textarea scrolling only after six visible rows", async () => {
		restoreTextareaMetrics = mockComposerTextareaMetrics();
		const sixRows = ["one", "two", "three", "four", "five", "six"].join("\n");
		const sevenRows = [
			"one",
			"two",
			"three",
			"four",
			"five",
			"six",
			"seven",
		].join("\n");
		renderComposer({
			initialDraft: sixRows,
		});

		const textarea = screen.getByRole("textbox") as HTMLTextAreaElement;
		await waitFor(() => {
			expect(textarea.style.height).toBe("152px");
			expect(textarea.style.overflowY).toBe("hidden");
		});

		fireEvent.change(textarea, { target: { value: sevenRows } });

		await waitFor(() => {
			expect(textarea.style.height).toBe("152px");
			expect(textarea.style.overflowY).toBe("auto");
		});
	});

	it("omits the auto-compact toggle when the change handler is not provided", () => {
		renderComposer();
		expect(
			screen.queryByRole("button", { name: "Auto-compact before sending" }),
		).toBeNull();
	});

	it("reflects auto-compact enabled state and toggles via the handler", () => {
		const onAutoCompactEnabledChange = vi.fn();
		renderComposer({
			autoCompactEnabled: true,
			onAutoCompactEnabledChange,
		});
		const toggle = screen.getByRole("button", {
			name: "Auto-compact before sending",
		});
		expect(toggle.getAttribute("aria-pressed")).toBe("true");
		fireEvent.click(toggle);
		expect(onAutoCompactEnabledChange).toHaveBeenCalledWith(false);
	});

	it("renders the context indicator beside composer actions", () => {
		renderComposer({
			contextMeter: (
				<WebComposerContextIndicator
					status="ready"
					hasUserMessage
					snapshot={{
						modelId: "gpt-5.5",
						budget: {
							effectiveInputBudgetTokens: 100_000,
							estimatedTokens: 42_000,
							knownProviderBudget: true,
							pressure: "medium",
							pressurePercent: 42,
							remainingTokens: 58_000,
						},
					}}
				/>
			),
		});

		expect(
			screen.getByRole("button", {
				name: "Context meter: 58 percent context remaining for gpt-5.5",
			}),
		).toBeTruthy();
	});
});
