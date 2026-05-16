import { decode as msgpackDecode } from "@msgpack/msgpack";

export interface RuntimeEnvelope {
	v: number;
	seq: number;
	computer_id: string;
	session_id: string;
	kind: "snapshot" | "event" | "presence" | "session_added" | "session_removed";
	payload: unknown;
}

/**
 * Frames arrive on Phoenix Channels as `{ from_device_id, from_kind, direction, payload }`
 * where `payload` is `{ encoding: 'msgpack.base64url', envelope: '<b64>', seq, kind }`.
 * Decode `envelope` into the typed shape mirrored from `xero-remote-bridge`.
 */
export function decodeRelayFrame(rawFrame: unknown): RuntimeEnvelope | null {
	if (!rawFrame || typeof rawFrame !== "object") return null;
	const frame = rawFrame as { payload?: unknown };
	const payload = frame.payload as
		| { encoding?: string; envelope?: string }
		| undefined;
	if (
		!payload ||
		payload.encoding !== "msgpack.base64url" ||
		typeof payload.envelope !== "string"
	) {
		return null;
	}
	const bytes = base64UrlToBytes(payload.envelope);
	const decoded = msgpackDecode(bytes) as RuntimeEnvelope;
	return decoded;
}

function base64UrlToBytes(input: string): Uint8Array {
	const normalized = input.replace(/-/g, "+").replace(/_/g, "/");
	const padded = normalized + "=".repeat((4 - (normalized.length % 4)) % 4);
	const binary =
		typeof atob === "function"
			? atob(padded)
			: Buffer.from(padded, "base64").toString("binary");
	const bytes = new Uint8Array(binary.length);
	for (let i = 0; i < binary.length; i += 1) {
		bytes[i] = binary.charCodeAt(i);
	}
	return bytes;
}
