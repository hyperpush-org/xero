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
		| "resolve_operator_action"
		| "cancel_run"
		| "context_snapshot"
		| "list_sessions"
		| "list_projects"
		| "stage_attachment"
		| "discard_attachment"
		| "update_session_controls"
		| "fetch_runtime_media_artifact";
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

export function requestThemeSnapshot(
	channel: Channel,
	options: {
		computerId: string;
		deviceId: string;
	},
): void {
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: "__theme__",
		device_id: options.deviceId,
		kind: "session_attached",
		payload: { lastSeq: 0 },
	});
}

export function requestContextSnapshot(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		requestId: string;
		providerId?: string | null;
		modelId?: string | null;
		pendingPrompt?: string | null;
	},
): void {
	const payload: Record<string, unknown> = {
		requestId: options.requestId,
	};
	if (options.providerId) payload.providerId = options.providerId;
	if (options.modelId) payload.modelId = options.modelId;
	if (options.pendingPrompt) payload.pendingPrompt = options.pendingPrompt;

	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "context_snapshot",
		payload,
	});
}

export function requestRuntimeMediaArtifact(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		artifactId: string;
	},
): void {
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "fetch_runtime_media_artifact",
		payload: { artifactId: options.artifactId },
	});
}

export function requestStartSession(
	channel: Channel,
	options: {
		computerId: string;
		projectId: string;
		deviceId: string;
		title?: string | null;
		prompt?: string | null;
		sessionKind?: "standard" | "computer_use";
		agent?: string | null;
	},
): void {
	const payload: Record<string, unknown> = {
		projectId: options.projectId,
		prompt: options.prompt ?? "",
	};
	if (options.title?.trim()) payload.title = options.title.trim();
	if (options.sessionKind) payload.sessionKind = options.sessionKind;
	if (options.agent?.trim()) payload.agent = options.agent.trim();

	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: "__new__",
		device_id: options.deviceId,
		kind: "start_session",
		payload,
	});
}

export function requestStageAttachment(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		attachmentId: string;
		originalName: string;
		mediaType: string;
		bytesBase64: string;
		runId?: string | null;
	},
): void {
	const payload: Record<string, unknown> = {
		attachmentId: options.attachmentId,
		originalName: options.originalName,
		mediaType: options.mediaType,
		bytesBase64: options.bytesBase64,
	};
	if (options.runId) payload.runId = options.runId;
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "stage_attachment",
		payload,
	});
}

export function requestDiscardAttachment(
	channel: Channel,
	options: {
		computerId: string;
		sessionId: string;
		deviceId: string;
		attachmentId: string;
		absolutePath: string;
	},
): void {
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: options.computerId,
		session_id: options.sessionId,
		device_id: options.deviceId,
		kind: "discard_attachment",
		payload: {
			attachmentId: options.attachmentId,
			absolutePath: options.absolutePath,
		},
	});
}

export function requestSessionArchive(
	channel: Channel,
	options: {
		computerId: string;
		projectId: string;
		sessionId: string;
		agentSessionId: string;
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
			agentSessionId: options.agentSessionId,
			remoteSessionId: options.sessionId,
		},
	});
}
