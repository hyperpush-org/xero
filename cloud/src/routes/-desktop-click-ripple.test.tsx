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

	it("shows the pulsing logo while a requested stream is connecting", () => {
		const push = vi.fn();
		const channel = {
			on: vi.fn(() => "frame-ref"),
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
		const toolbar = within(desktop).getByRole("toolbar", {
			name: "Desktop stream controls",
		});
		fireEvent.click(within(toolbar).getByRole("button", { name: /start/i }));

		expect(screen.getByText("Connecting stream")).toBeTruthy();
		expect(desktop.querySelector(".xero-loading-ring")).toBeTruthy();
		expect(desktop.querySelector(".xero-loading-breathe")).toBeTruthy();
		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				kind: "computer_use_stream_request",
				payload: expect.objectContaining({
					quality: "balanced",
					streamToken: "stream-token-1",
				}),
			}),
		);
	});

	it("tells the user to stop the other cloud connection before starting here", async () => {
		const push = vi.fn((_event: string, frame: Record<string, unknown>) => {
			const response = {
				command: {
					schema: "xero.remote_command_outcome.v1",
					clientCommandId: frame.clientCommandId,
					clientSeq: frame.clientSeq,
					kind: frame.kind,
					outcome: "rejected",
					priority: frame.priority,
					reason: "computer_use_connection_already_active",
					message:
						"Computer Use is already connected from another device or location. Stop the running connection first to use it here.",
					sentAt: frame.sentAt,
				},
			};

			return {
				receive(status: string, callback: (payload: unknown) => void) {
					if (status === "error") queueMicrotask(() => callback(response));
					return this;
				},
			};
		});
		const channel = {
			on: vi.fn(() => "frame-ref"),
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
		const toolbar = within(desktop).getByRole("toolbar", {
			name: "Desktop stream controls",
		});
		fireEvent.click(within(toolbar).getByRole("button", { name: /start/i }));

		expect(await screen.findByText("Already in use")).toBeTruthy();
		expect(
			screen.getByText(
				"Stop the running connection in the other cloud app before using it here.",
			),
		).toBeTruthy();
		expect(
			within(toolbar).getByRole("button", { name: /start/i }),
		).toBeTruthy();
	});

	it("retries the desktop stream request when connecting stalls before media arrives", () => {
		vi.useFakeTimers();
		try {
			const push = vi.fn();
			const channel = {
				on: vi.fn(() => "frame-ref"),
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
			const toolbar = within(desktop).getByRole("toolbar", {
				name: "Desktop stream controls",
			});
			fireEvent.click(within(toolbar).getByRole("button", { name: /start/i }));

			expect(streamRequestCalls(push)).toHaveLength(1);

			act(() => {
				vi.advanceTimersByTime(7_000);
			});

			expect(streamRequestCalls(push)).toHaveLength(2);
		} finally {
			vi.useRealTimers();
		}
	});

	it("keeps a healthy live WebRTC stream mounted when decoded frames are quiet", async () => {
		vi.useFakeTimers();
		const peerConnections = installMockPeerConnection();
		try {
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
			const toolbar = within(desktop).getByRole("toolbar", {
				name: "Desktop stream controls",
			});
			fireEvent.click(within(toolbar).getByRole("button", { name: /start/i }));
			expect(streamRequestCalls(push)).toHaveLength(1);

			await act(async () => {
				frameHandler?.(
					relayFrame({
						schema: "xero.computer_use_stream_offer.v1",
						streamId: "stream-1",
						payload: {
							type: "offer",
							sdp: "v=0\r\n",
						},
						desktop: {
							stream: {
								status: "starting",
								transport: "web_rtc",
								quality: "balanced",
							},
						},
					}),
				);
				await Promise.resolve();
			});
			expect(peerConnections.instances).toHaveLength(1);

			act(() => {
				peerConnections.instances[0]?.emitTrack();
			});
			expect(desktop.querySelector("video")).toBeTruthy();

			push.mockClear();
			act(() => {
				vi.advanceTimersByTime(9_000);
			});

			expect(streamRequestCalls(push)).toHaveLength(0);
			expect(desktop.querySelector("video")).toBeTruthy();
			expect(screen.queryByText("Connecting stream")).toBeNull();
		} finally {
			peerConnections.restore();
			vi.useRealTimers();
		}
	});

	it("queues ICE candidates that arrive before the WebRTC offer is applied", async () => {
		const peerConnections = installMockPeerConnection();
		try {
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

			await act(async () => {
				frameHandler?.(
					relayFrame({
						schema: "xero.computer_use_stream_ice_candidate.v1",
						streamId: "stream-1",
						payload: {
							candidate: {
								candidate: "candidate:1",
								sdpMid: "0",
								sdpMLineIndex: 0,
							},
						},
					}),
				);
				frameHandler?.(
					relayFrame({
						schema: "xero.computer_use_stream_offer.v1",
						streamId: "stream-1",
						payload: {
							type: "offer",
							sdp: "v=0\r\n",
						},
						desktop: {
							stream: {
								status: "starting",
								transport: "web_rtc",
								quality: "balanced",
							},
						},
					}),
				);
				await Promise.resolve();
			});

			expect(peerConnections.instances).toHaveLength(1);
			expect(peerConnections.instances[0]?.addedIceCandidates).toEqual([
				expect.objectContaining({ candidate: "candidate:1" }),
			]);
		} finally {
			peerConnections.restore();
		}
	});

	it("shows a click ripple where manual input lands on the streamed desktop", async () => {
		const { desktop, image, push } = await renderManualDesktopViewport();
		image.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		desktop.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		push.mockClear();

		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 160,
			clientY: 90,
			detail: 1,
			pointerId: 7,
		});
		expect(push).not.toHaveBeenCalled();
		fireEvent.pointerUp(desktop, {
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

	it("does not send manual input before the desktop grants control", async () => {
		const { desktop, image, push, toolbar } = await renderManualDesktopViewport(
			{
				grantManual: false,
			},
		);
		image.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		desktop.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		push.mockClear();

		expect(
			within(toolbar)
				.getByRole("button", { name: /requesting/i })
				.hasAttribute("disabled"),
		).toBe(true);

		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 160,
			clientY: 90,
			detail: 1,
			pointerId: 71,
		});
		fireEvent.pointerUp(desktop, {
			button: 0,
			clientX: 160,
			clientY: 90,
			detail: 1,
			pointerId: 71,
		});

		expect(push).not.toHaveBeenCalled();
		expect(desktop.querySelector(".desktop-click-ripple")).toBeNull();
	});

	it("shows a visible manual-control denial and keeps input disabled", async () => {
		const { desktop, frameHandler, image, manualControlId, push, toolbar } =
			await renderManualDesktopViewport({ grantManual: false });
		image.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		desktop.getBoundingClientRect = () => domRect(0, 0, 640, 360);

		act(() => {
			frameHandler?.(
				relayFrame({
					schema: "xero.computer_use_manual_control_request.v1",
					ok: false,
					outcome: "rejected",
					manualControlId,
					streamId: "stream-1",
				}),
			);
		});

		await waitFor(() => {
			expect(
				within(toolbar).getByRole("button", { name: /retry/i }),
			).toBeTruthy();
		});

		push.mockClear();
		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 160,
			clientY: 90,
			detail: 1,
			pointerId: 72,
		});
		fireEvent.pointerUp(desktop, {
			button: 0,
			clientX: 160,
			clientY: 90,
			detail: 1,
			pointerId: 72,
		});

		expect(push).not.toHaveBeenCalled();
	});

	it("maps manual input against the painted stream area when object-contain letterboxes the media", async () => {
		const { desktop, image, push } = await renderManualDesktopViewport();
		image.getBoundingClientRect = () => domRect(0, 0, 640, 640);
		desktop.getBoundingClientRect = () => domRect(0, 0, 640, 640);
		push.mockClear();

		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 160,
			clientY: 410,
			detail: 1,
			pointerId: 8,
		});
		fireEvent.pointerUp(desktop, {
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

	it("keeps a small pointer move within click slop as one click", async () => {
		const { desktop, image, push } = await renderManualDesktopViewport();
		image.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		desktop.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		push.mockClear();

		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 160,
			clientY: 90,
			detail: 1,
			pointerId: 9,
		});
		fireEvent.pointerMove(desktop, {
			buttons: 1,
			clientX: 164,
			clientY: 94,
			pointerId: 9,
		});
		fireEvent.pointerUp(desktop, {
			button: 0,
			clientX: 164,
			clientY: 94,
			detail: 1,
			pointerId: 9,
		});

		expect(push).toHaveBeenCalledTimes(1);
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

	it("sends one left-button drag after movement exceeds click slop", async () => {
		const { desktop, image, push } = await renderManualDesktopViewport();
		image.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		desktop.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		push.mockClear();

		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 160,
			clientY: 90,
			detail: 1,
			pointerId: 10,
		});
		fireEvent.pointerMove(desktop, {
			buttons: 1,
			clientX: 320,
			clientY: 180,
			pointerId: 10,
		});
		fireEvent.pointerUp(desktop, {
			button: 0,
			clientX: 320,
			clientY: 180,
			detail: 1,
			pointerId: 10,
		});

		expect(push).toHaveBeenCalledTimes(1);
		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				kind: "computer_use_manual_control_input",
				payload: expect.objectContaining({
					action: "mouse_drag",
					x: 320,
					y: 180,
					toX: 640,
					toY: 360,
					sourceWidth: 1280,
					sourceHeight: 720,
					button: "left",
				}),
			}),
		);
		expect(
			push.mock.calls.some(
				([, frame]) =>
					(frame as { payload?: { action?: string } }).payload?.action ===
					"mouse_click",
			),
		).toBe(false);
		expect(desktop.querySelector(".desktop-click-ripple")).toBeNull();
	});

	it("maps drag source and target against the painted stream area", async () => {
		const { desktop, image, push } = await renderManualDesktopViewport();
		image.getBoundingClientRect = () => domRect(0, 0, 640, 640);
		desktop.getBoundingClientRect = () => domRect(0, 0, 640, 640);
		push.mockClear();

		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 160,
			clientY: 410,
			detail: 1,
			pointerId: 11,
		});
		fireEvent.pointerMove(desktop, {
			buttons: 1,
			clientX: 480,
			clientY: 230,
			pointerId: 11,
		});
		fireEvent.pointerUp(desktop, {
			button: 0,
			clientX: 480,
			clientY: 230,
			detail: 1,
			pointerId: 11,
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				kind: "computer_use_manual_control_input",
				payload: expect.objectContaining({
					action: "mouse_drag",
					x: 320,
					y: 540,
					toX: 960,
					toY: 180,
					sourceWidth: 1280,
					sourceHeight: 720,
				}),
			}),
		);
	});

	it("does not send click or drag when a pointer gesture is cancelled", async () => {
		const { desktop, image, push } = await renderManualDesktopViewport();
		image.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		desktop.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		push.mockClear();

		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 160,
			clientY: 90,
			detail: 1,
			pointerId: 12,
		});
		fireEvent.pointerMove(desktop, {
			buttons: 1,
			clientX: 320,
			clientY: 180,
			pointerId: 12,
		});
		fireEvent.pointerCancel(desktop, {
			pointerId: 12,
		});
		fireEvent.pointerUp(desktop, {
			button: 0,
			clientX: 320,
			clientY: 180,
			pointerId: 12,
		});

		expect(push).not.toHaveBeenCalled();
		expect(desktop.querySelector(".desktop-click-ripple")).toBeNull();
	});

	it("ignores zero-distance manual scroll events before they reach the desktop", async () => {
		const { desktop, push } = await renderManualDesktopViewport();
		push.mockClear();

		fireEvent.wheel(desktop, { deltaX: 0.2, deltaY: -0.2 });
		expect(push).not.toHaveBeenCalled();

		fireEvent.wheel(desktop, { deltaX: 0.2, deltaY: 2.8 });
		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				kind: "computer_use_manual_control_input",
				payload: expect.objectContaining({
					action: "scroll",
					deltaX: 0,
					deltaY: 3,
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
		expect(manualInputCalls(push)).toHaveLength(0);

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

	it("keeps mobile taps from opening the keyboard until the toolbar button is used", async () => {
		const { desktop, image, keyboard, push, toolbar } =
			await renderManualDesktopViewport({
				presentation: {
					isMobile: true,
					override: "mobile",
					rotateDesktop: false,
				},
			});
		image.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		desktop.getBoundingClientRect = () => domRect(0, 0, 640, 360);
		push.mockClear();
		const promptToolbar = within(desktop).getByRole("toolbar", {
			name: "Desktop prompt controls",
		});
		expect(promptToolbar.getAttribute("style")).toContain("bottom:");
		expect(
			within(toolbar).queryByRole("form", { name: "Computer Use prompt" }),
		).toBeNull();
		expect(
			within(promptToolbar).getByRole("form", {
				name: "Computer Use prompt",
			}),
		).toBeTruthy();

		fireEvent.pointerDown(desktop, {
			button: 0,
			clientX: 160,
			clientY: 90,
			pointerId: 31,
			pointerType: "touch",
		});
		fireEvent.pointerUp(desktop, {
			button: 0,
			clientX: 160,
			clientY: 90,
			detail: 1,
			pointerId: 31,
			pointerType: "touch",
		});

		expect(document.activeElement).not.toBe(keyboard);
		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				kind: "computer_use_manual_control_input",
				payload: expect.objectContaining({
					action: "mouse_click",
					x: 320,
					y: 180,
				}),
			}),
		);

		push.mockClear();
		fireEvent.click(
			within(toolbar).getByRole("button", { name: "Show desktop keyboard" }),
		);

		await waitFor(() => {
			expect(document.activeElement).toBe(keyboard);
		});
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
	fireEvent.pointerUp(context.desktop, {
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
	grantManual = true,
	presentation = {
		isMobile: false,
		override: "desktop",
		rotateDesktop: false,
	},
}: {
	grantManual?: boolean;
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
	const manualRequest = push.mock.calls
		.map(
			([, frame]) =>
				frame as { kind?: string; payload?: { manualControlId?: string } },
		)
		.find((frame) => frame.kind === "computer_use_manual_control_request");
	const manualControlId = manualRequest?.payload?.manualControlId;
	expect(manualControlId).toBeTruthy();
	if (grantManual) {
		act(() => {
			frameHandler?.(
				relayFrame({
					schema: "xero.computer_use_manual_control_request.v1",
					ok: true,
					outcome: "executed",
					manualControlId,
					streamId: "stream-1",
				}),
			);
		});
	}
	await waitFor(() => {
		if (grantManual) {
			expect(
				within(toolbar).getByRole("button", { name: /release/i }),
			).toBeTruthy();
		} else {
			expect(
				within(toolbar).getByRole("button", { name: /requesting/i }),
			).toBeTruthy();
		}
	});

	return {
		desktop,
		frameHandler,
		image,
		keyboard,
		manualControlId,
		push,
		toolbar,
	};
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

function installMockPeerConnection() {
	const previous = globalThis.RTCPeerConnection;
	const instances: MockPeerConnection[] = [];

	class MockPeerConnection {
		addedIceCandidates: RTCIceCandidateInit[] = [];
		connectionState: RTCPeerConnectionState = "connected";
		iceConnectionState: RTCIceConnectionState = "connected";
		localDescription: RTCSessionDescriptionInit | null = null;
		onconnectionstatechange: ((event: Event) => void) | null = null;
		ondatachannel: ((event: RTCDataChannelEvent) => void) | null = null;
		onicecandidate: ((event: RTCPeerConnectionIceEvent) => void) | null = null;
		ontrack: ((event: RTCTrackEvent) => void) | null = null;
		remoteDescription: RTCSessionDescriptionInit | null = null;

		constructor() {
			instances.push(this);
		}

		async addIceCandidate(candidate: RTCIceCandidateInit) {
			this.addedIceCandidates.push(candidate);
			return undefined;
		}

		close() {
			this.connectionState = "closed";
			this.iceConnectionState = "closed";
			this.onconnectionstatechange?.(new Event("connectionstatechange"));
		}

		async createAnswer(): Promise<RTCSessionDescriptionInit> {
			return { type: "answer", sdp: "v=0\r\nmock-answer" };
		}

		emitTrack() {
			this.ontrack?.({
				streams: [{} as MediaStream],
			} as unknown as RTCTrackEvent);
		}

		getReceivers() {
			return [
				{
					track: { stop: vi.fn() },
				},
			];
		}

		async setLocalDescription(description: RTCSessionDescriptionInit) {
			this.localDescription = description;
		}

		async setRemoteDescription(description: RTCSessionDescriptionInit) {
			this.remoteDescription = description;
		}
	}

	Object.defineProperty(globalThis, "RTCPeerConnection", {
		configurable: true,
		writable: true,
		value: MockPeerConnection,
	});

	return {
		instances,
		restore: () => {
			Object.defineProperty(globalThis, "RTCPeerConnection", {
				configurable: true,
				writable: true,
				value: previous,
			});
		},
	};
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

function streamRequestCalls(push: ReturnType<typeof vi.fn>) {
	return push.mock.calls.filter(
		([, frame]) =>
			(frame as { kind?: string }).kind === "computer_use_stream_request",
	);
}

function manualInputCalls(push: ReturnType<typeof vi.fn>) {
	return push.mock.calls.filter(
		([, frame]) =>
			(frame as { kind?: string }).kind === "computer_use_manual_control_input",
	);
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
