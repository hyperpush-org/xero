import type { ConversationTurn } from "@xero/ui/components/transcript/conversation-section";
import { create } from "zustand";

type ActionTurn = Extract<ConversationTurn, { kind: "action" }>;

export interface VisibleSessionSummary {
	computerId: string;
	sessionId: string;
	title: string;
	lastActivityAt: string | null;
	computerName: string | null;
}

export interface SessionTranscript {
	turns: ConversationTurn[];
	lastSeq: number;
	isLive: boolean;
	availableAgents: { id: string; label: string }[];
	availableModels: { id: string; label: string }[];
	currentAgentId: string | null;
	currentModelId: string | null;
}

interface SessionStoreState {
	visibleSessions: VisibleSessionSummary[];
	visibleSessionsVersion: number;
	visibleSessionsByComputerVersion: Record<string, number>;
	transcripts: Record<string, SessionTranscript>;
	setVisibleSessions: (sessions: VisibleSessionSummary[]) => void;
	clearVisibleSessionsForComputers: (computerIds: readonly string[]) => void;
	replaceVisibleSessionsForComputer: (
		computerId: string,
		sessions: VisibleSessionSummary[],
	) => void;
	upsertVisibleSession: (summary: VisibleSessionSummary) => void;
	removeVisibleSession: (computerId: string, sessionId: string) => void;
	replaceWithSnapshot: (key: string, transcript: SessionTranscript) => void;
	appendTurn: (key: string, turn: ConversationTurn, seq: number) => void;
	updateControls: (
		key: string,
		controls: { agentId?: string | null; modelId?: string | null },
	) => void;
	setLive: (key: string, isLive: boolean) => void;
}

export const sessionKey = (computerId: string, sessionId: string) =>
	`${computerId}:${sessionId}`;

function visibleSessionsEqual(
	left: readonly VisibleSessionSummary[],
	right: readonly VisibleSessionSummary[],
): boolean {
	if (left === right) return true;
	if (left.length !== right.length) return false;
	return left.every((current, index) => {
		const next = right[index];
		return (
			current.computerId === next.computerId &&
			current.sessionId === next.sessionId &&
			current.title === next.title &&
			current.lastActivityAt === next.lastActivityAt &&
			current.computerName === next.computerName
		);
	});
}

function sortVisibleSessions(
	sessions: readonly VisibleSessionSummary[],
): VisibleSessionSummary[] {
	return [...sessions].sort(compareVisibleSessions);
}

function compareVisibleSessions(
	left: VisibleSessionSummary,
	right: VisibleSessionSummary,
): number {
	return (
		safeTimestamp(right.lastActivityAt) - safeTimestamp(left.lastActivityAt) ||
		left.title.localeCompare(right.title) ||
		left.sessionId.localeCompare(right.sessionId)
	);
}

function safeTimestamp(value: string | null): number {
	if (!value) return 0;
	const timestamp = Date.parse(value);
	return Number.isFinite(timestamp) ? timestamp : 0;
}

function pruneHiddenTranscripts(
	transcripts: Record<string, SessionTranscript>,
	visibleSessions: readonly VisibleSessionSummary[],
): Record<string, SessionTranscript> {
	const visibleKeys = new Set(
		visibleSessions.map((session) =>
			sessionKey(session.computerId, session.sessionId),
		),
	);
	const entries = Object.entries(transcripts).filter(([key]) =>
		visibleKeys.has(key),
	);
	return entries.length === Object.keys(transcripts).length
		? transcripts
		: Object.fromEntries(entries);
}

function omitComputerVersions(
	versions: Record<string, number>,
	computerIds: ReadonlySet<string>,
): Record<string, number> {
	let changed = false;
	const next = { ...versions };
	for (const computerId of computerIds) {
		if (computerId in next) {
			delete next[computerId];
			changed = true;
		}
	}
	return changed ? next : versions;
}

export const useSessionStore = create<SessionStoreState>((set) => ({
	visibleSessions: [],
	visibleSessionsVersion: 0,
	visibleSessionsByComputerVersion: {},
	transcripts: {},
	setVisibleSessions: (sessions) =>
		set((state) => {
			const nextSessions = sortVisibleSessions(sessions);
			if (visibleSessionsEqual(state.visibleSessions, nextSessions)) {
				return state;
			}
			return {
				visibleSessions: nextSessions,
				visibleSessionsVersion: state.visibleSessionsVersion + 1,
				transcripts: pruneHiddenTranscripts(state.transcripts, nextSessions),
			};
		}),
	clearVisibleSessionsForComputers: (computerIds) =>
		set((state) => {
			const clearSet = new Set(computerIds);
			if (clearSet.size === 0) return state;
			const nextSessions = state.visibleSessions.filter(
				(session) => !clearSet.has(session.computerId),
			);
			const nextByComputerVersion = omitComputerVersions(
				state.visibleSessionsByComputerVersion,
				clearSet,
			);
			if (visibleSessionsEqual(state.visibleSessions, nextSessions)) {
				return nextByComputerVersion === state.visibleSessionsByComputerVersion
					? state
					: { visibleSessionsByComputerVersion: nextByComputerVersion };
			}
			return {
				visibleSessions: nextSessions,
				visibleSessionsVersion: state.visibleSessionsVersion + 1,
				visibleSessionsByComputerVersion: nextByComputerVersion,
				transcripts: pruneHiddenTranscripts(state.transcripts, nextSessions),
			};
		}),
	replaceVisibleSessionsForComputer: (computerId, sessions) =>
		set((state) => {
			const nextSessions = sortVisibleSessions([
				...state.visibleSessions.filter(
					(session) => session.computerId !== computerId,
				),
				...sessions,
			]);
			const nextVersion = state.visibleSessionsVersion + 1;
			return {
				visibleSessions: visibleSessionsEqual(
					state.visibleSessions,
					nextSessions,
				)
					? state.visibleSessions
					: nextSessions,
				visibleSessionsVersion: nextVersion,
				visibleSessionsByComputerVersion: {
					...state.visibleSessionsByComputerVersion,
					[computerId]: nextVersion,
				},
				transcripts: pruneHiddenTranscripts(state.transcripts, nextSessions),
			};
		}),
	upsertVisibleSession: (summary) =>
		set((state) => {
			const nextSessions = sortVisibleSessions([
				summary,
				...state.visibleSessions.filter(
					(session) =>
						!(
							session.computerId === summary.computerId &&
							session.sessionId === summary.sessionId
						),
				),
			]);
			const nextVersion = state.visibleSessionsVersion + 1;
			return {
				visibleSessions: nextSessions,
				visibleSessionsVersion: nextVersion,
				visibleSessionsByComputerVersion: {
					...state.visibleSessionsByComputerVersion,
					[summary.computerId]: nextVersion,
				},
			};
		}),
	removeVisibleSession: (computerId, sessionId) =>
		set((state) => {
			const nextSessions = state.visibleSessions.filter(
				(session) =>
					!(
						session.computerId === computerId && session.sessionId === sessionId
					),
			);
			if (visibleSessionsEqual(state.visibleSessions, nextSessions)) {
				return state;
			}
			return {
				visibleSessions: nextSessions,
				visibleSessionsVersion: state.visibleSessionsVersion + 1,
				transcripts: pruneHiddenTranscripts(state.transcripts, nextSessions),
			};
		}),
	replaceWithSnapshot: (key, transcript) =>
		set((state) => ({
			transcripts: { ...state.transcripts, [key]: transcript },
		})),
	appendTurn: (key, turn, seq) =>
		set((state) => {
			const current = state.transcripts[key];
			if (!current) return state;
			return {
				transcripts: {
					...state.transcripts,
					[key]: {
						...current,
						turns: appendConversationTurn(current.turns, turn),
						lastSeq: Math.max(current.lastSeq, seq),
					},
				},
			};
		}),
	updateControls: (key, controls) =>
		set((state) => {
			const current = state.transcripts[key];
			if (!current) return state;
			const currentAgentId =
				controls.agentId === undefined
					? current.currentAgentId
					: controls.agentId;
			const currentModelId =
				controls.modelId === undefined
					? current.currentModelId
					: controls.modelId;
			return {
				transcripts: {
					...state.transcripts,
					[key]: {
						...current,
						currentAgentId: currentAgentId ?? null,
						currentModelId: currentModelId ?? null,
						availableModels: ensureOption(
							current.availableModels,
							currentModelId ?? null,
						),
					},
				},
			};
		}),
	setLive: (key, isLive) =>
		set((state) => {
			const current = state.transcripts[key];
			if (!current) return state;
			return {
				transcripts: { ...state.transcripts, [key]: { ...current, isLive } },
			};
		}),
}));

function appendConversationTurn(
	turns: readonly ConversationTurn[],
	turn: ConversationTurn,
): ConversationTurn[] {
	const previous = turns.at(-1);
	if (
		turn.kind === "message" &&
		turn.role === "assistant" &&
		previous?.kind === "message" &&
		previous.role === "assistant"
	) {
		return [
			...turns.slice(0, -1),
			{
				...previous,
				text: `${previous.text}${turn.text}`,
				sequence: turn.sequence,
			},
		];
	}

	if (turn.kind === "thinking" && previous?.kind === "thinking") {
		return [
			...turns.slice(0, -1),
			{
				...previous,
				text: `${previous.text}${turn.text}`,
				sequence: turn.sequence,
			},
		];
	}

	if (turn.kind === "action") {
		const existingIndex = turns.findIndex(
			(existing) =>
				existing.kind === "action" && existing.toolCallId === turn.toolCallId,
		);
		if (existingIndex >= 0) {
			return turns.map((existing, index) =>
				index === existingIndex && existing.kind === "action"
					? mergeActionTurn(existing, turn)
					: existing,
			);
		}
	}

	return [...turns, turn];
}

function ensureOption(
	options: readonly { id: string; label: string }[],
	id: string | null,
): { id: string; label: string }[] {
	if (!id || options.some((option) => option.id === id)) {
		return [...options];
	}
	return [{ id, label: id }, ...options];
}

function mergeActionTurn(
	existing: ActionTurn,
	incoming: ActionTurn,
): ActionTurn {
	return {
		...existing,
		title: incoming.title || existing.title,
		detail: incoming.detail || existing.detail,
		detailRows: mergeActionRows(existing.detailRows, incoming.detailRows),
		state: incoming.state ?? existing.state,
	};
}

function mergeActionRows(
	existing: ActionTurn["detailRows"],
	incoming: ActionTurn["detailRows"],
): ActionTurn["detailRows"] {
	const merged = existing.map((row) => ({ ...row }));
	const seen = new Set(merged.map((row) => `${row.label}\u0000${row.value}`));
	for (const row of incoming) {
		const key = `${row.label}\u0000${row.value}`;
		if (seen.has(key)) continue;
		seen.add(key);
		merged.push(row);
	}
	return merged;
}
