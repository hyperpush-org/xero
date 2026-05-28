/** @vitest-environment jsdom */

import { encode as msgpackEncode } from "@msgpack/msgpack";
import {
	act,
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import type { Channel } from "phoenix";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ComputerUseDesktopViewport } from "./sessions.$computerId.$sessionId";

describe("ComputerUseDesktopViewport click feedback", () => {
	beforeEach(() => {
		Object.defineProperty(URL, "createObjectURL", {
			configurable: true,
			writable: true,
			value: vi.fn(() => "blob:desktop-frame"),
		});
		Object.defineProperty(URL, "revokeObjectURL", {
			configurable: true,
			writable: true,
			value: vi.fn(),
		});
		Object.defineProperty(HTMLElement.prototype, "setPointerCapture", {
			configurable: true,
			writable: true,
			value: vi.fn(),
		});
		Object.defineProperty(HTMLElement.prototype, "releasePointerCapture", {
			configurable: true,
			writable: true,
			value: vi.fn(),
		});
	});

	afterEach(() => {
		cleanup();
		vi.clearAllMocks();
	});

	it("shows a click ripple where manual input lands on the streamed desktop", async () => {
		const { desktop, image, push } = await renderManualDesktopViewport();
		image.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		desktop.getBoundingClientRect = () => domRect(0, 0, 640, 360);

		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 160,
			clientY: 90,
			detail: 1,
			pointerId: 7,
		});

		const ripple = desktop.querySelector(
			".desktop-click-ripple",
		) as HTMLElement | null;
		expect(ripple).toBeTruthy();
		expect(ripple?.style.left).toBe("160px");
		expect(ripple?.style.top).toBe("90px");
		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				kind: "computer_use_manual_control_input",
				payload: expect.objectContaining({
					action: "mouse_click",
					x: 320,
					y: 180,
					sourceWidth: 1280,
					sourceHeight: 720,
				}),
			}),
		);
	});

	it("maps manual input against the painted stream area when object-contain letterboxes the media", async () => {
		const { desktop, image, push } = await renderManualDesktopViewport();
		image.getBoundingClientRect = () => domRect(0, 0, 640, 640);
		desktop.getBoundingClientRect = () => domRect(0, 0, 640, 640);

		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 160,
			clientY: 410,
			detail: 1,
			pointerId: 8,
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				kind: "computer_use_manual_control_input",
				payload: expect.objectContaining({
					action: "mouse_click",
					x: 320,
					y: 540,
					sourceWidth: 1280,
					sourceHeight: 720,
				}),
			}),
		);
	});
});

async function renderManualDesktopViewport() {
	let frameHandler: ((rawFrame: unknown) => void) | null = null;
	const push = vi.fn();
	const channel = {
		on: vi.fn((event: string, handler: (rawFrame: unknown) => void) => {
			if (event === "frame") frameHandler = handler;
			return "frame-ref";
		}),
		off: vi.fn(),
		push,
	} as unknown as Channel;

	render(
		<ComputerUseDesktopViewport
			channel={channel}
			computerId="desktop-1"
			deviceId="web-1"
			iceServers={[]}
			isAgentWorking={false}
			isOnline
			onPromptSubmit={vi.fn()}
			previewUrl={null}
			presentation={{
				isMobile: false,
				override: "desktop",
				rotateDesktop: false,
			}}
			sessionId="session-1"
			streamRunId="run-1"
			streamToken="stream-token-1"
		/>,
	);

	const desktop = screen.getByLabelText("Desktop");
	expect(frameHandler).toBeTruthy();
	act(() => {
		frameHandler?.(
			relayFrame({
				schema: "xero.computer_use_stream_frame.v1",
				streamId: "stream-1",
				desktop: {
					stream: {
						status: "degraded",
						transport: "screenshot_fallback",
						quality: "balanced",
					},
				},
				desktopFrame: {
					ok: true,
					mediaType: "image/png",
					bytesBase64: "iVBORw0KGgo=",
				},
			}),
		);
	});

	const image = await within(desktop).findByRole("img", { name: "Desktop" });
	Object.defineProperty(image, "naturalWidth", {
		configurable: true,
		value: 1280,
	});
	Object.defineProperty(image, "naturalHeight", {
		configurable: true,
		value: 720,
	});

	const toolbar = within(desktop).getByRole("toolbar", {
		name: "Desktop stream controls",
	});
	await waitFor(() => {
		expect(
			within(toolbar)
				.getByRole("button", { name: /manual/i })
				.hasAttribute("disabled"),
		).toBe(false);
	});
	fireEvent.click(within(toolbar).getByRole("button", { name: /manual/i }));
	await waitFor(() => {
		expect(
			within(toolbar).getByRole("button", { name: /release/i }),
		).toBeTruthy();
	});

	return { desktop, image, push };
}

function relayFrame(payload: unknown) {
	const bytes = msgpackEncode({
		v: 1,
		seq: 1,
		computer_id: "desktop-1",
		session_id: "session-1",
		kind: "event",
		payload,
	});
	let binary = "";
	for (const byte of bytes) binary += String.fromCharCode(byte);
	return {
		payload: {
			encoding: "msgpack.base64url",
			envelope: btoa(binary)
				.replace(/\+/g, "-")
				.replace(/\//g, "_")
				.replace(/=+$/, ""),
		},
	};
}

function domRect(left: number, top: number, width: number, height: number) {
	return {
		bottom: top + height,
		height,
		left,
		right: left + width,
		toJSON: () => ({}),
		top,
		width,
		x: left,
		y: top,
	} as DOMRect;
}
