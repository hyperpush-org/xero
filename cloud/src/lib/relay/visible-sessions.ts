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
	  }
	| {
			kind: "remove";
			computerId: string;
			sessionId: string;
	  };

export function remoteVisibleSessionUpdateFromEnvelope(
	envelope: RuntimeEnvelope,
	devices: readonly AccountDevice[] = [],
): RemoteVisibleSessionUpdate | null {
	if (envelope.session_id !== "__sessions__") return null;
	if (!isRecord(envelope.payload)) return null;

	const schema = stringField(envelope.payload, "schema");
	const computerName = desktopNameForId(devices, envelope.computer_id);

	if (schema === "xero.remote_sessions.v1") {
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
		const projectId = isRecord(envelope.payload.result)
			? stringField(envelope.payload.result, "projectId", "project_id")
			: stringField(envelope.payload, "projectId", "project_id");
		const projectName = isRecord(envelope.payload.result)
			? stringField(envelope.payload.result, "projectName", "project_name")
			: stringField(envelope.payload, "projectName", "project_name");
		const summary = summaryFromRemoteSession(
			envelope.computer_id,
			computerName,
			projectId,
			projectName,
			session,
		);
		return summary?.remoteVisible ? { kind: "upsert", summary } : null;
	}

	if (
		schema === "xero.remote_session_removed.v1" ||
		envelope.kind === "session_removed"
	) {
		const sessionId = removedSessionId(envelope.payload);
		return sessionId
			? { kind: "remove", computerId: envelope.computer_id, sessionId }
			: null;
	}

	return null;
}

function removedSessionId(payload: Record<string, unknown>): string | null {
	return (
		stringField(
			payload,
			"remoteSessionId",
			"remote_session_id",
			"sessionId",
			"session_id",
			"agentSessionId",
			"agent_session_id",
		) ??
		(isRecord(payload.session)
			? stringField(
					payload.session,
					"remoteSessionId",
					"remote_session_id",
					"agentSessionId",
					"agent_session_id",
					"sessionId",
					"session_id",
				)
			: null)
	);
}

function summaryFromRemoteVisibleEntry(
	computerId: string,
	computerName: string | null,
	entry: unknown,
): VisibleSessionSummary | null {
	if (!isRecord(entry)) return null;
	const projectId =
		stringField(entry, "projectId", "project_id") ??
		(isRecord(entry.session)
			? stringField(entry.session, "projectId", "project_id")
			: null);
	const projectName =
		stringField(entry, "projectName", "project_name") ??
		(isRecord(entry.session)
			? stringField(entry.session, "projectName", "project_name")
			: null);
	return summaryFromRemoteSession(
		computerId,
		computerName,
		projectId,
		projectName,
		entry.session,
	);
}

function summaryFromRemoteSession(
	computerId: string,
	computerName: string | null,
	projectId: string | null,
	projectName: string | null,
	session: unknown,
): VisibleSessionSummary | null {
	if (!isRecord(session)) return null;
	const agentSessionId = stringField(
		session,
		"agentSessionId",
		"agent_session_id",
		"sessionId",
		"session_id",
	);
	const sessionId =
		stringField(session, "remoteSessionId", "remote_session_id") ??
		stringField(session, "sessionId", "session_id") ??
		agentSessionId;
	const resolvedProjectId =
		projectId ?? stringField(session, "projectId", "project_id");
	if (!sessionId || !agentSessionId || !resolvedProjectId) return null;
	const resolvedProjectName =
		projectName ?? stringField(session, "projectName", "project_name");
	const sessionKind = sessionKindField(session);
	const summary = {
		computerId,
		sessionId,
		agentSessionId,
		projectId: resolvedProjectId,
		projectName: resolvedProjectName,
		sessionKind,
		isComputerUse: sessionKind === "computer_use",
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
		remoteVisible:
			booleanField(session, "remoteVisible", "remote_visible") ?? true,
	};
	return summary.remoteVisible ? summary : null;
}

function sessionKindField(
	record: Record<string, unknown>,
): "standard" | "computer_use" {
	const value = stringField(record, "sessionKind", "session_kind");
	return value === "computer_use" ? "computer_use" : "standard";
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

function booleanField(
	record: Record<string, unknown>,
	...keys: string[]
): boolean | null {
	for (const key of keys) {
		const value = record[key];
		if (typeof value === "boolean") return value;
	}
	return null;
}
