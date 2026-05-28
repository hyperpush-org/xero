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
import { Dialog } from "@xero/ui/components/ui/dialog";
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

	it("zooms the mobile desktop stream with pinch gestures and maps the next tap through the zoomed media", async () => {
		const { desktop, image, push } = await renderManualDesktopViewport({
			presentation: {
				isMobile: true,
				override: "mobile",
				rotateDesktop: false,
			},
		});
		image.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		desktop.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		const mediaLayer = image.parentElement as HTMLElement;
		push.mockClear();

		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 220,
			clientY: 180,
			pointerId: 21,
			pointerType: "touch",
		});
		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 420,
			clientY: 180,
			pointerId: 22,
			pointerType: "touch",
		});
		fireEvent.pointerMove(desktop, {
			clientX: 120,
			clientY: 180,
			pointerId: 21,
			pointerType: "touch",
		});
		fireEvent.pointerMove(desktop, {
			clientX: 520,
			clientY: 180,
			pointerId: 22,
			pointerType: "touch",
		});

		await waitFor(() => {
			expect(mediaLayer.style.transform).toContain("scale(2)");
		});
		expect(push).not.toHaveBeenCalled();

		fireEvent.pointerUp(desktop, {
			clientX: 120,
			clientY: 180,
			pointerId: 21,
			pointerType: "touch",
		});
		fireEvent.pointerUp(desktop, {
			clientX: 520,
			clientY: 180,
			pointerId: 22,
			pointerType: "touch",
		});

		image.getBoundingClientRect = () => domRect(-320, -180, 1280, 720);
		push.mockClear();

		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 160,
			clientY: 90,
			pointerId: 23,
			pointerType: "touch",
		});
		expect(push).not.toHaveBeenCalled();
		fireEvent.pointerUp(desktop, {
			button: 0,
			clientX: 160,
			clientY: 90,
			detail: 1,
			pointerId: 23,
			pointerType: "touch",
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				kind: "computer_use_manual_control_input",
				payload: expect.objectContaining({
					action: "mouse_click",
					x: 480,
					y: 270,
					sourceWidth: 1280,
					sourceHeight: 720,
				}),
			}),
		);
	});

	it("arms keyboard capture after a manual desktop click and sends text input", async () => {
		const { desktop, keyboard, push } = await armManualKeyboardCapture();

		fireTextInput(keyboard, "hi");

		await waitFor(() => {
			expect(push).toHaveBeenCalledWith(
				"frame",
				expect.objectContaining({
					kind: "computer_use_manual_control_input",
					payload: expect.objectContaining({
						action: "type_text",
						text: "hi",
					}),
				}),
			);
		});

		expect(desktop.textContent).toContain("Keyboard captured");
	});

	it("does not send keyboard input before the desktop media surface captures it", async () => {
		const { keyboard, push } = await renderManualDesktopViewport();
		push.mockClear();

		fireTextInput(keyboard, "x");

		await new Promise((resolve) => window.setTimeout(resolve, 30));
		expect(push).not.toHaveBeenCalled();
	});

	it.each([
		["Enter", "Enter"],
		["Tab", "Tab"],
		["Backspace", "Backspace"],
		["Delete", "Delete"],
		["Escape", "Escape"],
		["ArrowLeft", "ArrowLeft"],
		["Home", "Home"],
		["End", "End"],
		["PageUp", "PageUp"],
		["PageDown", "PageDown"],
		["F12", "F12"],
	])("sends %s as a key_press payload", async (domKey, brokerKey) => {
		const { keyboard, push } = await armManualKeyboardCapture();

		fireEvent.keyDown(keyboard, { key: domKey });

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				payload: expect.objectContaining({
					action: "key_press",
					key: brokerKey,
				}),
			}),
		);
	});

	it("sends modifier shortcuts as normalized hotkey payloads", async () => {
		const { keyboard, push } = await armManualKeyboardCapture();

		fireEvent.keyDown(keyboard, { key: "a", metaKey: true });

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				payload: expect.objectContaining({
					action: "hotkey",
					keys: ["command", "a"],
				}),
			}),
		);
	});

	it("uses text events for shifted printable characters", async () => {
		const { keyboard, push } = await armManualKeyboardCapture();

		fireEvent.keyDown(keyboard, { key: "A", shiftKey: true });
		fireTextInput(keyboard, "A");

		await waitFor(() => {
			expect(push).toHaveBeenCalledWith(
				"frame",
				expect.objectContaining({
					payload: expect.objectContaining({
						action: "type_text",
						text: "A",
					}),
				}),
			);
		});
		expect(
			push.mock.calls.some(
				([, frame]) =>
					(frame as { payload?: { action?: string } }).payload?.action ===
					"hotkey",
			),
		).toBe(false);
	});

	it("sends composed text once when IME composition completes", async () => {
		const { keyboard, push } = await armManualKeyboardCapture();

		fireEvent.compositionStart(keyboard);
		fireEvent.keyDown(keyboard, { key: "Dead" });
		fireEvent.compositionEnd(keyboard, { data: "é" });
		fireBeforeInput(keyboard, "é", "insertFromComposition");

		await waitFor(() => {
			const typeTextCalls = push.mock.calls.filter(
				([, frame]) =>
					(frame as { payload?: { action?: string } }).payload?.action ===
					"type_text",
			);
			expect(typeTextCalls).toHaveLength(1);
			expect(typeTextCalls[0]?.[1]).toEqual(
				expect.objectContaining({
					payload: expect.objectContaining({
						text: "é",
					}),
				}),
			);
		});
	});

	it("sends paste_text only from an explicit paste event payload", async () => {
		const { keyboard, push } = await armManualKeyboardCapture();
		const getData = vi.fn(() => "pasted text");

		fireEvent.paste(keyboard, {
			clipboardData: {
				getData,
			},
		});

		expect(getData).toHaveBeenCalledWith("text/plain");
		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				payload: expect.objectContaining({
					action: "paste_text",
					text: "pasted text",
				}),
			}),
		);
		expect(
			push.mock.calls.some(
				([, frame]) =>
					(frame as { payload?: { action?: string } }).payload?.action ===
					"type_text",
			),
		).toBe(false);
	});

	it("disarms keyboard capture when toolbar controls are used", async () => {
		const { keyboard, push, toolbar } = await armManualKeyboardCapture();

		fireEvent.pointerDown(toolbar, { button: 0, pointerId: 11 });
		fireEvent.keyDown(keyboard, { key: "Enter" });

		expect(push).not.toHaveBeenCalled();
	});

	it("disarms keyboard capture when manual control is released", async () => {
		const { keyboard, push, toolbar } = await armManualKeyboardCapture();

		fireEvent.click(within(toolbar).getByRole("button", { name: /release/i }));
		push.mockClear();
		fireTextInput(keyboard, "x");

		await new Promise((resolve) => window.setTimeout(resolve, 30));
		expect(push).not.toHaveBeenCalled();
	});
});

async function armManualKeyboardCapture() {
	const context = await renderManualDesktopViewport();
	context.image.getBoundingClientRect = () => domRect(0, 0, 640, 360);
	context.desktop.getBoundingClientRect = () => domRect(0, 0, 640, 360);
	context.push.mockClear();

	fireEvent.pointerDown(context.desktop, {
		button: 0,
		clientX: 160,
		clientY: 90,
		detail: 1,
		pointerId: 10,
	});

	await waitFor(() => {
		expect(document.activeElement).toBe(context.keyboard);
	});
	context.push.mockClear();
	return context;
}

async function renderManualDesktopViewport({
	presentation = {
		isMobile: false,
		override: "desktop",
		rotateDesktop: false,
	},
}: {
	presentation?: {
		isMobile: boolean;
		override: "auto" | "desktop" | "mobile";
		rotateDesktop: boolean;
	};
} = {}) {
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

	const viewport = (
		<ComputerUseDesktopViewport
			channel={channel}
			computerId="desktop-1"
			deviceId="web-1"
			iceServers={[]}
			isAgentWorking={false}
			isOnline
			onPromptSubmit={vi.fn()}
			previewUrl={null}
			presentation={presentation}
			sessionId="session-1"
			streamRunId="run-1"
			streamToken="stream-token-1"
		/>
	);

	render(presentation.isMobile ? <Dialog open>{viewport}</Dialog> : viewport);

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
	const keyboard = screen.getByLabelText(
		"Desktop keyboard passthrough",
	) as HTMLTextAreaElement;
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

	return { desktop, image, keyboard, push, toolbar };
}

function fireBeforeInput(
	element: HTMLElement,
	data: string,
	inputType = "insertText",
) {
	const event = new Event("beforeinput", {
		bubbles: true,
		cancelable: true,
	}) as InputEvent;
	Object.defineProperty(event, "data", { value: data });
	Object.defineProperty(event, "inputType", { value: inputType });
	Object.defineProperty(event, "isComposing", { value: false });
	fireEvent(element, event);
}

function fireTextInput(element: HTMLTextAreaElement, text: string) {
	element.value = text;
	fireEvent.input(element, {
		target: { value: text },
	});
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
