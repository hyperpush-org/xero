import type { RuntimeEnvelope } from "./envelope";
import type { RemoteProjectSummary } from "./session-store";

export interface RemoteProjectsUpdate {
	computerId: string;
	projects: RemoteProjectSummary[];
}

export function remoteProjectsUpdateFromEnvelope(
	envelope: RuntimeEnvelope,
): RemoteProjectsUpdate | null {
	if (envelope.session_id !== "__projects__") return null;
	if (!isRecord(envelope.payload)) return null;
	if (envelope.payload.schema !== "xero.remote_projects.v1") return null;

	const rawProjects = Array.isArray(envelope.payload.projects)
		? envelope.payload.projects
		: [];
	const projects: RemoteProjectSummary[] = [];
	for (const entry of rawProjects) {
		if (!isRecord(entry)) continue;
		const projectId = stringField(entry, "projectId", "project_id");
		if (!projectId) continue;
		const projectName = stringField(entry, "projectName", "project_name");
		projects.push({
			computerId: envelope.computer_id,
			projectId,
			projectName,
		});
	}
	return { computerId: envelope.computer_id, projects };
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
