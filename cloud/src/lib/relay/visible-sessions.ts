import type { AccountDevice } from "../auth/session";
import type { RuntimeEnvelope } from "./envelope";
import type { VisibleSessionSummary } from "./session-store";

export type RemoteVisibleSessionUpdate =
	| {
			kind: "replace";
			computerId: string;
			sessions: VisibleSessionSummary[];
	  }
	| {
			kind: "upsert";
			summary: VisibleSessionSummary;
	  };

export function remoteVisibleSessionUpdateFromEnvelope(
	envelope: RuntimeEnvelope,
	devices: readonly AccountDevice[] = [],
): RemoteVisibleSessionUpdate | null {
	if (envelope.session_id !== "__sessions__") return null;
	if (!isRecord(envelope.payload)) return null;

	const schema = stringField(envelope.payload, "schema");
	const computerName = desktopNameForId(devices, envelope.computer_id);

	if (schema === "xero.remote_visible_sessions.v1") {
		const entries = Array.isArray(envelope.payload.sessions)
			? envelope.payload.sessions
			: [];
		return {
			kind: "replace",
			computerId: envelope.computer_id,
			sessions: entries
				.map((entry) =>
					summaryFromRemoteVisibleEntry(
						envelope.computer_id,
						computerName,
						entry,
					),
				)
				.filter(
					(summary): summary is VisibleSessionSummary => summary !== null,
				),
		};
	}

	if (
		schema === "xero.remote_session_added.v1" ||
		schema === "xero.remote_session_started.v1"
	) {
		const session = isRecord(envelope.payload.result)
			? envelope.payload.result.session
			: envelope.payload.session;
		const summary = summaryFromRemoteSession(
			envelope.computer_id,
			computerName,
			session,
		);
		return summary ? { kind: "upsert", summary } : null;
	}

	return null;
}

function summaryFromRemoteVisibleEntry(
	computerId: string,
	computerName: string | null,
	entry: unknown,
): VisibleSessionSummary | null {
	if (!isRecord(entry)) return null;
	return summaryFromRemoteSession(computerId, computerName, entry.session);
}

function summaryFromRemoteSession(
	computerId: string,
	computerName: string | null,
	session: unknown,
): VisibleSessionSummary | null {
	if (!isRecord(session)) return null;
	const sessionId = stringField(session, "agentSessionId", "agent_session_id");
	if (!sessionId) return null;
	return {
		computerId,
		sessionId,
		title: stringField(session, "title") ?? "New Chat",
		lastActivityAt:
			stringField(
				session,
				"lastActivityAt",
				"last_activity_at",
				"updatedAt",
				"updated_at",
			) ?? null,
		computerName,
	};
}

function desktopNameForId(
	devices: readonly AccountDevice[],
	computerId: string,
): string | null {
	return devices.find((device) => device.id === computerId)?.name ?? null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function stringField(
	record: Record<string, unknown>,
	...keys: string[]
): string | null {
	for (const key of keys) {
		const value = record[key];
		if (typeof value !== "string") continue;
		const trimmed = value.trim();
		if (trimmed) return trimmed;
	}
	return null;
}
