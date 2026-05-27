import { beforeEach, describe, expect, it, vi } from "vitest";

import {
	answerComputerUseStreamOffer,
	disconnectRelay,
	getRelaySocket,
	heartbeatComputerUseManualControl,
	type InboundCommand,
	pushInboundCommand,
	requestComputerUseStream,
	requestComputerUseStreamKeyframe,
	requestRunCancel,
	requestSessionArchive,
	requestSessionSnapshot,
	requestStartSession,
	requestThemeSnapshot,
	sendComputerUseStreamIceCandidate,
	setComputerUseStreamQuality,
} from "./relay-client";

const { MockSocket, socketInstances } = vi.hoisted(() => {
	class MockSocket {
		readonly url: string;
		readonly opts: { params?: { token?: string } };
		state: "connecting" | "open" | "closing" | "closed" = "connecting";
		connect = vi.fn();
		disconnect = vi.fn(() => {
			this.state = "closed";
		});

		constructor(url: string, opts: { params?: { token?: string } } = {}) {
			this.url = url;
			this.opts = opts;
			socketInstances.push(this);
		}

		connectionState() {
			return this.state;
		}

		isConnected() {
			return this.state === "open";
		}
	}

	const socketInstances: MockSocket[] = [];
	return { MockSocket, socketInstances };
});
type MockSocketInstance = InstanceType<typeof MockSocket>;

vi.mock("phoenix", () => ({
	Socket: MockSocket,
}));

vi.mock("../server-url", () => ({
	getServerUrl: () => "http://relay.test",
}));

beforeEach(() => {
	disconnectRelay();
	socketInstances.length = 0;
	vi.clearAllMocks();
});

describe("getRelaySocket", () => {
	it("reuses the in-flight socket when a refreshed relay token arrives", () => {
		const first = getRelaySocket("token-a") as unknown as MockSocketInstance;
		const second = getRelaySocket("token-b") as unknown as MockSocketInstance;

		expect(second).toBe(first);
		expect(first.disconnect).not.toHaveBeenCalled();
		expect(socketInstances).toHaveLength(1);
	});

	it("opens a replacement socket once the previous socket is closed", () => {
		const first = getRelaySocket("token-a") as unknown as MockSocketInstance;
		first.state = "closed";

		const second = getRelaySocket("token-b") as unknown as MockSocketInstance;

		expect(second).not.toBe(first);
		expect(first.disconnect).toHaveBeenCalledTimes(1);
		expect(socketInstances).toHaveLength(2);
	});
});

describe("pushInboundCommand", () => {
	it("sends the command as the Phoenix frame payload", () => {
		const push = vi.fn();
		const command: InboundCommand = {
			v: 1,
			seq: 42,
			computer_id: "desktop-1",
			session_id: "__sessions__",
			device_id: "web-1",
			kind: "list_sessions",
			payload: {},
		};

		pushInboundCommand({ push } as never, command);

		expect(push).toHaveBeenCalledWith("frame", command);
	});

	it("requests a fresh snapshot using the desktop session-attached command", () => {
		const push = vi.fn();

		requestSessionSnapshot({ push } as never, {
			computerId: "desktop-1",
			sessionId: "session-1",
			deviceId: "web-1",
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				v: 1,
				computer_id: "desktop-1",
				session_id: "session-1",
				device_id: "web-1",
				kind: "session_attached",
				payload: { lastSeq: 0 },
			}),
		);
	});

	it("requests the desktop-selected theme from the read-only theme channel", () => {
		const push = vi.fn();

		requestThemeSnapshot({ push } as never, {
			computerId: "desktop-1",
			deviceId: "web-1",
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				v: 1,
				computer_id: "desktop-1",
				session_id: "__theme__",
				device_id: "web-1",
				kind: "session_attached",
				payload: { lastSeq: 0 },
			}),
		);
	});

	it("requests a desktop-side session archive from the session-list channel", () => {
		const push = vi.fn();

		requestSessionArchive({ push } as never, {
			computerId: "desktop-1",
			projectId: "project-1",
			sessionId: "project:9:project-1session-1",
			agentSessionId: "session-1",
			deviceId: "web-1",
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				v: 1,
				computer_id: "desktop-1",
				session_id: "__sessions__",
				device_id: "web-1",
				kind: "archive_session",
				payload: {
					projectId: "project-1",
					agentSessionId: "session-1",
					remoteSessionId: "project:9:project-1session-1",
				},
			}),
		);
	});

	it("requests a desktop-side new session from the new-session control channel", () => {
		const push = vi.fn();

		requestStartSession({ push } as never, {
			computerId: "desktop-1",
			projectId: "project-1",
			deviceId: "web-1",
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				v: 1,
				computer_id: "desktop-1",
				session_id: "__new__",
				device_id: "web-1",
				kind: "start_session",
				payload: {
					projectId: "project-1",
					prompt: "",
				},
			}),
		);
	});

	it("requests a desktop-side Computer Use session when selected", () => {
		const push = vi.fn();

		requestStartSession({ push } as never, {
			computerId: "desktop-1",
			projectId: "project-1",
			deviceId: "web-1",
			sessionKind: "computer_use",
			agent: "computer_use",
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				kind: "start_session",
				payload: {
					projectId: "project-1",
					prompt: "",
					sessionKind: "computer_use",
					agent: "computer_use",
				},
			}),
		);
	});

	it("answers a Computer Use desktop stream offer through the brokered stream channel", () => {
		const push = vi.fn();

		answerComputerUseStreamOffer({ push } as never, {
			computerId: "desktop-1",
			sessionId: "session-1",
			deviceId: "web-1",
			streamId: "stream-1",
			answer: { type: "answer", sdp: "v=0" },
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				computer_id: "desktop-1",
				session_id: "session-1",
				device_id: "web-1",
				kind: "computer_use_stream_answer",
				payload: {
					streamId: "stream-1",
					type: "answer",
					sdp: "v=0",
				},
			}),
		);
	});

	it("requests Computer Use streams with relay-issued ICE servers", () => {
		const push = vi.fn();

		requestComputerUseStream({ push } as never, {
			computerId: "desktop-1",
			sessionId: "session-1",
			deviceId: "web-1",
			displayId: "display-2",
			quality: "high",
			runId: "run-1",
			streamToken: "stream-token-1",
			iceServers: [
				{
					urls: ["turn:turn.example.test:3478"],
					username: "user",
					credential: "pass",
				},
			],
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				computer_id: "desktop-1",
				session_id: "session-1",
				device_id: "web-1",
				kind: "computer_use_stream_request",
				payload: {
					displayId: "display-2",
					quality: "high",
					includeCursor: true,
					runId: "run-1",
					streamToken: "stream-token-1",
					iceServers: [
						{
							urls: ["turn:turn.example.test:3478"],
							username: "user",
							credential: "pass",
						},
					],
				},
			}),
		);
	});

	it("sends Computer Use desktop stream ICE candidates through the broker", () => {
		const push = vi.fn();

		sendComputerUseStreamIceCandidate({ push } as never, {
			computerId: "desktop-1",
			sessionId: "session-1",
			deviceId: "web-1",
			streamId: "stream-1",
			candidate: {
				candidate: "candidate:1",
				sdpMid: "0",
				sdpMLineIndex: 0,
			},
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				computer_id: "desktop-1",
				session_id: "session-1",
				device_id: "web-1",
				kind: "computer_use_stream_ice_candidate",
				payload: {
					streamId: "stream-1",
					candidate: {
						candidate: "candidate:1",
						sdpMid: "0",
						sdpMLineIndex: 0,
					},
				},
			}),
		);
	});

	it("updates Computer Use stream quality through the broker", () => {
		const push = vi.fn();

		setComputerUseStreamQuality({ push } as never, {
			computerId: "desktop-1",
			sessionId: "session-1",
			deviceId: "web-1",
			streamId: "stream-1",
			quality: "low",
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				computer_id: "desktop-1",
				session_id: "session-1",
				device_id: "web-1",
				kind: "computer_use_stream_set_quality",
				payload: {
					streamId: "stream-1",
					quality: "low",
				},
			}),
		);
	});

	it("requests Computer Use stream keyframes through the broker", () => {
		const push = vi.fn();

		requestComputerUseStreamKeyframe({ push } as never, {
			computerId: "desktop-1",
			sessionId: "session-1",
			deviceId: "web-1",
			streamId: "stream-1",
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				computer_id: "desktop-1",
				session_id: "session-1",
				device_id: "web-1",
				kind: "computer_use_stream_request_keyframe",
				payload: {
					streamId: "stream-1",
				},
			}),
		);
	});

	it("requests run cancellation through the broker", () => {
		const push = vi.fn();

		requestRunCancel({ push } as never, {
			computerId: "desktop-1",
			sessionId: "session-1",
			deviceId: "web-1",
			reason: "cloud_emergency_stop",
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				computer_id: "desktop-1",
				session_id: "session-1",
				device_id: "web-1",
				kind: "cancel_run",
				payload: {
					reason: "cloud_emergency_stop",
				},
			}),
		);
	});

	it("refreshes Computer Use manual-control leases through the broker", () => {
		const push = vi.fn();

		heartbeatComputerUseManualControl({ push } as never, {
			computerId: "desktop-1",
			sessionId: "session-1",
			deviceId: "web-1",
			manualControlId: "manual-1",
			runId: "run-1",
			streamToken: "stream-token-1",
		});

		expect(push).toHaveBeenCalledWith(
			"frame",
			expect.objectContaining({
				computer_id: "desktop-1",
				session_id: "session-1",
				device_id: "web-1",
				kind: "computer_use_manual_control_heartbeat",
				payload: {
					manualControlId: "manual-1",
					reason: "manual_cloud_control_heartbeat",
					runId: "run-1",
					streamToken: "stream-token-1",
				},
			}),
		);
	});
});
