import { type Channel, Socket } from "phoenix";

import { getServerUrl } from "../server-url";

export interface InboundCommand {
	v: number;
	seq: number;
	computer_id: string;
	session_id?: string;
	device_id?: string;
	kind:
		| "send_message"
		| "start_session"
		| "resolve_operator_action"
		| "cancel_run"
		| "list_sessions";
	payload: unknown;
}

let socket: Socket | null = null;

/**
 * Lazily open the singleton browser → relay WebSocket. The relay URL is derived
 * from `XERO_SERVER_URL` (http → ws, https → wss) and the connection is
 * authenticated with a short-lived JWT obtained server-side from
 * `/api/relay/token/refresh`.
 */
export function getRelaySocket(token: string): Socket {
	if (socket?.isConnected()) return socket;
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
): Channel {
	const channel = socketInstance.channel(`session:${computerId}:${sessionId}`, {
		last_seq: lastSeq ?? 0,
	});
	channel.join();
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
	channel.push("frame", {
		encoding: "json",
		direction: "inbound",
		payload: command,
	});
}
