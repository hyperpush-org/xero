/** @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ConversationTurn } from "@xero/ui/components/transcript/conversation-section";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
	getCloudConversationScrollKey,
	isCloudConversationNearBottom,
	useConversationAutoFollow,
} from "./use-conversation-auto-follow";

beforeEach(() => {
	vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
		callback(0);
		return 1;
	});
	vi.stubGlobal("cancelAnimationFrame", vi.fn());
});

afterEach(() => {
	cleanup();
	vi.unstubAllGlobals();
});

describe("useConversationAutoFollow", () => {
	it("classifies scroll positions near the bottom", () => {
		expect(
			isCloudConversationNearBottom({
				scrollTop: 420,
				scrollHeight: 1_000,
				clientHeight: 500,
			}),
		).toBe(true);
		expect(
			isCloudConversationNearBottom({
				scrollTop: 300,
				scrollHeight: 1_000,
				clientHeight: 500,
			}),
		).toBe(false);
		expect(
			isCloudConversationNearBottom({
				scrollTop: 0,
				scrollHeight: 320,
				clientHeight: 500,
			}),
		).toBe(true);
	});

	it("keys streaming assistant text changes", () => {
		expect(getCloudConversationScrollKey([assistantTurn("Hi")], true)).not.toBe(
			getCloudConversationScrollKey([assistantTurn("Hi there")], true),
		);
	});

	it("follows live transcript growth while pinned to latest", () => {
		const { rerender } = render(
			<AutoFollowHarness turns={[assistantTurn("Hello")]} />,
		);
		const viewport = screen.getByTestId("viewport");
		const metrics = mockScrollMetrics(viewport, {
			scrollTop: 420,
			scrollHeight: 1_000,
			clientHeight: 500,
		});
		fireEvent.scroll(viewport);

		metrics.set({ scrollHeight: 1_250 });
		rerender(
			<AutoFollowHarness turns={[assistantTurn("Hello, still streaming")]} />,
		);

		expect(viewport.scrollTop).toBe(1_250);
	});

	it("pauses follow mode when the user scrolls away", () => {
		const { rerender } = render(
			<AutoFollowHarness turns={[assistantTurn("Hello")]} />,
		);
		const viewport = screen.getByTestId("viewport");
		const metrics = mockScrollMetrics(viewport, {
			scrollTop: 100,
			scrollHeight: 1_000,
			clientHeight: 500,
		});
		fireEvent.scroll(viewport);

		metrics.set({ scrollHeight: 1_300 });
		rerender(
			<AutoFollowHarness turns={[assistantTurn("Hello, still streaming")]} />,
		);

		expect(viewport.scrollTop).toBe(100);
	});

	it("resumes follow mode when requested", () => {
		render(<AutoFollowHarness turns={[assistantTurn("Hello")]} />);
		const viewport = screen.getByTestId("viewport");
		const metrics = mockScrollMetrics(viewport, {
			scrollTop: 100,
			scrollHeight: 1_000,
			clientHeight: 500,
		});
		fireEvent.scroll(viewport);

		metrics.set({ scrollHeight: 1_300 });
		fireEvent.click(screen.getByRole("button", { name: "Follow latest" }));

		expect(viewport.scrollTop).toBe(1_300);
	});

	it("resets follow mode when switching sessions", () => {
		const { rerender } = render(
			<AutoFollowHarness
				sessionKey="desktop-1:session-1"
				turns={[assistantTurn("Hello")]}
			/>,
		);
		const viewport = screen.getByTestId("viewport");
		const metrics = mockScrollMetrics(viewport, {
			scrollTop: 100,
			scrollHeight: 1_000,
			clientHeight: 500,
		});
		fireEvent.scroll(viewport);

		metrics.set({ scrollHeight: 1_250 });
		rerender(
			<AutoFollowHarness
				sessionKey="desktop-1:session-1"
				turns={[assistantTurn("Hello, still streaming")]}
			/>,
		);
		expect(viewport.scrollTop).toBe(100);

		metrics.set({ scrollHeight: 1_500 });
		rerender(
			<AutoFollowHarness
				sessionKey="desktop-1:session-2"
				turns={[assistantTurn("Hello, still streaming")]}
			/>,
		);

		expect(viewport.scrollTop).toBe(1_500);
	});
});

function AutoFollowHarness({
	enabled = true,
	isLive = true,
	sessionKey = "desktop-1:session-1",
	turns,
}: {
	enabled?: boolean;
	isLive?: boolean;
	sessionKey?: string;
	turns: ConversationTurn[];
}) {
	const scroll = useConversationAutoFollow({
		enabled,
		isLive,
		sessionKey,
		turns,
	});

	return (
		<div
			data-testid="viewport"
			ref={scroll.viewportRef}
			onScroll={scroll.onScroll}
			onWheel={scroll.onWheel}
		>
			<div ref={scroll.contentRef}>
				{turns.map((turn) => (
					<p key={turn.id}>{turn.kind === "message" ? turn.text : turn.id}</p>
				))}
				<button type="button" onClick={scroll.followLatest}>
					Follow latest
				</button>
			</div>
		</div>
	);
}

function assistantTurn(text: string): ConversationTurn {
	return {
		id: "assistant-turn",
		kind: "message",
		role: "assistant",
		sequence: 1,
		text,
	};
}

function mockScrollMetrics(
	element: HTMLElement,
	initialMetrics: {
		scrollTop: number;
		scrollHeight: number;
		clientHeight: number;
	},
) {
	let scrollTop = initialMetrics.scrollTop;
	let scrollHeight = initialMetrics.scrollHeight;
	let clientHeight = initialMetrics.clientHeight;

	Object.defineProperties(element, {
		scrollTop: {
			configurable: true,
			get: () => scrollTop,
			set: (value: number) => {
				scrollTop = value;
			},
		},
		scrollHeight: {
			configurable: true,
			get: () => scrollHeight,
		},
		clientHeight: {
			configurable: true,
			get: () => clientHeight,
		},
	});

	return {
		set(nextMetrics: Partial<typeof initialMetrics>) {
			scrollTop = nextMetrics.scrollTop ?? scrollTop;
			scrollHeight = nextMetrics.scrollHeight ?? scrollHeight;
			clientHeight = nextMetrics.clientHeight ?? clientHeight;
		},
	};
}
