import { type Channel, Socket } from "phoenix";

import { getServerUrl } from "../server-url";

export interface InboundCommand {
	v: number;
	seq: number;
	computer_id: string;
	session_id?: string;
	device_id?: string;
	kind:
		| "session_attached"
		| "send_message"
		| "start_session"
		| "archive_session"
		| "set_session_visibility"
		| "resolve_operator_action"
		| "cancel_run"
		| "list_sessions"
		| "list_projects";
	payload: unknown;
}

let socket: Socket | null = null;

function socketIsReusable(socketInstance: Socket): boolean {
	const state = socketInstance.connectionState();
	return state === "connecting" || state === "open";
}

/**
 * Lazily open the singleton browser → relay WebSocket. The relay URL is derived
 * from `XERO_SERVER_URL` (http → ws, https → wss) and the connection is
 * authenticated with a short-lived JWT obtained server-side from
 * `/api/relay/token/refresh`.
 */
export function getRelaySocket(token: string): Socket {
	if (socket) {
		if (socketIsReusable(socket)) return socket;
		socket.disconnect();
	}
	const url = `${getServerUrl().replace(/^http/, "ws")}/socket/web`;
	socket = new Socket(url, { params: { token } });
	socket.connect();
	return socket;
}

export function disconnectRelay(): void {
	if (socket) {
		socket.disconnect();
		socket = null;
	}
}

export function joinAccountChannel(
	socketInstance: Socket,
	accountId: string,
): Channel {
	const channel = socketInstance.channel(`account:${accountId}`, {});
	channel.join();
	return channel;
}

export function joinSessionChannel(
	socketInstance: Socket,
	computerId: string,
	sessionId: string,
	lastSeq?: number,
	onJoined?: (channel: Channel) => void,
	onJoinError?: (payload: unknown) => void,
): Channel {
	const channel = socketInstance.channel(`session:${computerId}:${sessionId}`, {
		last_seq: lastSeq ?? 0,
	});
	const join = channel.join();
	if (onJoined) {
		join.receive("ok", () => onJoined(channel));
	}
	if (onJoinError) {
		join
			.receive("error", (payload) => onJoinError(payload))
			.receive("timeout", () => onJoinError({ reason: "timeout" }));
	}
	return channel;
}

/**
 * Send an inbound command frame on the given session channel. The payload is
 * forwarded verbatim to the owning desktop via the Phoenix relay.
 */
export function pushInboundCommand(
	channel: Channel,
	command: InboundCommand,
): void {
	channel.push("frame", command);
}

export function requestSessionSnapshot(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
	},
): void {
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "session_attached",
		payload: { lastSeq: 0 },
	});
}

export function requestSessionRemoteVisibility(
	channel: Channel,
	options: {
		computerId: string;
		projectId: string;
		sessionId: string;
		deviceId: string;
		visible: boolean;
	},
): void {
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: "__sessions__",
		device_id: options.deviceId,
		kind: "set_session_visibility",
		payload: {
			projectId: options.projectId,
			agentSessionId: options.sessionId,
			visible: options.visible,
		},
	});
}

export function requestSessionArchive(
	channel: Channel,
	options: {
		computerId: string;
		projectId: string;
		sessionId: string;
		deviceId: string;
	},
): void {
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: "__sessions__",
		device_id: options.deviceId,
		kind: "archive_session",
		payload: {
			projectId: options.projectId,
			agentSessionId: options.sessionId,
		},
	});
}
