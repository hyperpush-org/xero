import type { ConversationTurn } from "@xero/ui/components/transcript/conversation-section";
import type { RuntimeRunApprovalModeDto } from "@xero/ui/model/runtime";
import { create } from "zustand";
import { appendConversationTurn } from "./stream-projection";

export type SessionKind = "standard" | "computer_use";
export const REMOTE_COMPUTER_USE_SESSION_ID = "__computer_use__";
export const GLOBAL_COMPUTER_USE_PROJECT_ID = "global-computer-use";

export interface VisibleSessionSummary {
	computerId: string;
	sessionId: string;
	agentSessionId: string;
	projectId: string;
	projectName: string | null;
	sessionKind: SessionKind;
	isComputerUse: boolean;
	title: string;
	lastActivityAt: string | null;
	computerName: string | null;
	remoteVisible: boolean;
}

export interface RemoteProjectSummary {
	computerId: string;
	projectId: string;
	projectName: string | null;
}

export type SessionThinkingEffort =
	| "none"
	| "minimal"
	| "low"
	| "medium"
	| "high"
	| "x_high";
export type SessionApprovalMode = RuntimeRunApprovalModeDto;
export const DEFAULT_SESSION_APPROVAL_MODE: SessionApprovalMode = "suggest";

export interface SessionModelOption {
	id: string;
	label: string;
	modelId: string;
	providerId: string | null;
	providerLabel: string | null;
	providerProfileId: string | null;
	thinkingSupported: boolean;
	thinkingEffortOptions: SessionThinkingEffort[];
	defaultThinkingEffort: SessionThinkingEffort | null;
	inputModalities?: string[];
	supportedTypes?: string[];
	attachmentStatus?: string | null;
}

export interface SessionContextBudget {
	budgetTokens?: number | null;
	contextWindowTokens?: number | null;
	effectiveInputBudgetTokens?: number | null;
	maxOutputTokens?: number | null;
	outputReserveTokens?: number | null;
	safetyReserveTokens?: number | null;
	remainingTokens?: number | null;
	pressurePercent?: number | null;
	estimatedTokens?: number | null;
	estimationSource?: string | null;
	pressure?: "unknown" | "low" | "medium" | "high" | "over" | null;
	knownProviderBudget?: boolean | null;
	limitSource?: string | null;
	limitConfidence?: string | null;
	limitDiagnostic?: string | null;
	limitFetchedAt?: string | null;
}

export interface SessionContextSnapshot {
	snapshotId?: string | null;
	projectId?: string | null;
	agentSessionId?: string | null;
	runId?: string | null;
	providerId?: string | null;
	modelId?: string | null;
	generatedAt?: string | null;
	budget?: SessionContextBudget | null;
	contributors?: Array<{ kind?: string | null }>;
}

export interface SessionContextError {
	code?: string | null;
	message?: string | null;
	retryable?: boolean | null;
}

type RawSessionModelOption = Partial<
	Omit<
		SessionModelOption,
		| "modelId"
		| "providerId"
		| "providerLabel"
		| "providerProfileId"
		| "thinkingSupported"
		| "thinkingEffortOptions"
		| "defaultThinkingEffort"
		| "inputModalities"
		| "supportedTypes"
		| "attachmentStatus"
	>
> & {
	modelId?: string | null;
	providerId?: string | null;
	providerLabel?: string | null;
	providerProfileId?: string | null;
	thinkingSupported?: boolean | null;
	thinkingEffortOptions?: ReadonlyArray<string | null> | null;
	defaultThinkingEffort?: string | null;
	inputModalities?: ReadonlyArray<string | null> | null;
	supportedTypes?: ReadonlyArray<string | null> | null;
	attachmentStatus?: string | null;
};

export interface SessionTranscript {
	turns: ConversationTurn[];
	lastSeq: number;
	isLive: boolean;
	availableAgents: { id: string; label: string }[];
	availableModels: SessionModelOption[];
	currentAgentId: string | null;
	currentModelId: string | null;
	currentThinkingEffort: SessionThinkingEffort | null;
	currentApprovalMode?: SessionApprovalMode;
	currentAutoCompactEnabled: boolean;
	contextSnapshot?: SessionContextSnapshot | null;
	contextSnapshotError?: SessionContextError | null;
	contextSnapshotRequestId?: string | null;
}

interface SessionStoreState {
	visibleSessions: VisibleSessionSummary[];
	visibleSessionsVersion: number;
	visibleSessionsByComputerVersion: Record<string, number>;
	remoteProjectsByComputer: Record<string, RemoteProjectSummary[]>;
	onlineComputerIds: Record<string, true>;
	computerPresenceKnown: boolean;
	transcripts: Record<string, SessionTranscript>;
	setVisibleSessions: (sessions: VisibleSessionSummary[]) => void;
	clearVisibleSessionsForComputers: (computerIds: readonly string[]) => void;
	setOnlineComputers: (computerIds: readonly string[]) => void;
	resetComputerPresence: () => void;
	replaceVisibleSessionsForComputer: (
		computerId: string,
		sessions: VisibleSessionSummary[],
	) => void;
	replaceRemoteProjectsForComputer: (
		computerId: string,
		projects: RemoteProjectSummary[],
	) => void;
	clearRemoteProjectsForComputers: (computerIds: readonly string[]) => void;
	upsertVisibleSession: (summary: VisibleSessionSummary) => void;
	removeVisibleSession: (computerId: string, sessionId: string) => void;
	replaceWithSnapshot: (key: string, transcript: SessionTranscript) => void;
	appendTurn: (key: string, turn: ConversationTurn, seq: number) => void;
	updateControls: (
		key: string,
		controls: {
			agentId?: string | null;
			modelId?: string | null;
			rawModelId?: string | null;
			providerId?: string | null;
			providerProfileId?: string | null;
			thinkingEffort?: SessionThinkingEffort | null;
			approvalMode?: SessionApprovalMode;
			autoCompactEnabled?: boolean;
		},
	) => void;
	updateContextSnapshot: (
		key: string,
		update: {
			snapshot: SessionContextSnapshot | null;
			error: SessionContextError | null;
			requestId: string | null;
			seq?: number;
		},
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
			current.agentSessionId === next.agentSessionId &&
			current.projectId === next.projectId &&
			current.projectName === next.projectName &&
			current.sessionKind === next.sessionKind &&
			current.isComputerUse === next.isComputerUse &&
			current.title === next.title &&
			current.lastActivityAt === next.lastActivityAt &&
			current.computerName === next.computerName &&
			current.remoteVisible === next.remoteVisible
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

function pruneMissingTranscripts(
	transcripts: Record<string, SessionTranscript>,
	visibleSessions: readonly VisibleSessionSummary[],
): Record<string, SessionTranscript> {
	const visibleKeys = new Set(
		visibleSessions.map((session) =>
			sessionKey(session.computerId, session.sessionId),
		),
	);
	const entries = Object.entries(transcripts).filter(
		([key]) =>
			visibleKeys.has(key) ||
			key.endsWith(`:${REMOTE_COMPUTER_USE_SESSION_ID}`),
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

function sortRemoteProjects(
	projects: readonly RemoteProjectSummary[],
): RemoteProjectSummary[] {
	return [...projects].sort((left, right) => {
		const leftName = left.projectName ?? left.projectId;
		const rightName = right.projectName ?? right.projectId;
		return (
			leftName.localeCompare(rightName) ||
			left.projectId.localeCompare(right.projectId)
		);
	});
}

function remoteProjectsEqual(
	left: readonly RemoteProjectSummary[],
	right: readonly RemoteProjectSummary[],
): boolean {
	if (left === right) return true;
	if (left.length !== right.length) return false;
	return left.every((current, index) => {
		const next = right[index];
		return (
			current.computerId === next.computerId &&
			current.projectId === next.projectId &&
			current.projectName === next.projectName
		);
	});
}

function onlineComputerMap(
	computerIds: readonly string[],
): Record<string, true> {
	return Object.fromEntries(
		[...new Set(computerIds.filter((id) => id.trim()))].map((id) => [id, true]),
	);
}

function onlineComputersEqual(
	left: Record<string, true>,
	right: Record<string, true>,
): boolean {
	const leftKeys = Object.keys(left);
	const rightKeys = Object.keys(right);
	if (leftKeys.length !== rightKeys.length) return false;
	return leftKeys.every((key) => right[key]);
}

export const useSessionStore = create<SessionStoreState>((set) => ({
	visibleSessions: [],
	visibleSessionsVersion: 0,
	visibleSessionsByComputerVersion: {},
	remoteProjectsByComputer: {},
	onlineComputerIds: {},
	computerPresenceKnown: false,
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
			};
		}),
	replaceRemoteProjectsForComputer: (computerId, projects) =>
		set((state) => {
			const sortedNext = sortRemoteProjects(projects);
			const current = state.remoteProjectsByComputer[computerId];
			if (current && remoteProjectsEqual(current, sortedNext)) {
				return state;
			}
			return {
				remoteProjectsByComputer: {
					...state.remoteProjectsByComputer,
					[computerId]: sortedNext,
				},
			};
		}),
	clearRemoteProjectsForComputers: (computerIds) =>
		set((state) => {
			const clearSet = new Set(computerIds);
			if (clearSet.size === 0) return state;
			let changed = false;
			const next = { ...state.remoteProjectsByComputer };
			for (const computerId of clearSet) {
				if (computerId in next) {
					delete next[computerId];
					changed = true;
				}
			}
			return changed ? { remoteProjectsByComputer: next } : state;
		}),
	setOnlineComputers: (computerIds) =>
		set((state) => {
			const nextOnline = onlineComputerMap(computerIds);
			if (
				state.computerPresenceKnown &&
				onlineComputersEqual(state.onlineComputerIds, nextOnline)
			) {
				return state;
			}
			return {
				onlineComputerIds: nextOnline,
				computerPresenceKnown: true,
			};
		}),
	resetComputerPresence: () =>
		set((state) => {
			if (
				!state.computerPresenceKnown &&
				Object.keys(state.onlineComputerIds).length === 0
			) {
				return state;
			}
			return {
				onlineComputerIds: {},
				computerPresenceKnown: false,
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
				transcripts: pruneMissingTranscripts(state.transcripts, nextSessions),
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
				transcripts: pruneMissingTranscripts(state.transcripts, nextSessions),
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
			const nextVersion = state.visibleSessionsVersion + 1;
			return {
				visibleSessions: nextSessions,
				visibleSessionsVersion: nextVersion,
				visibleSessionsByComputerVersion: {
					...state.visibleSessionsByComputerVersion,
					[computerId]: nextVersion,
				},
				transcripts: pruneMissingTranscripts(state.transcripts, nextSessions),
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
			const currentThinkingEffort =
				controls.thinkingEffort === undefined
					? current.currentThinkingEffort
					: controls.thinkingEffort;
			const currentApprovalMode =
				controls.approvalMode === undefined
					? (current.currentApprovalMode ?? DEFAULT_SESSION_APPROVAL_MODE)
					: controls.approvalMode;
			const currentAutoCompactEnabled =
				controls.autoCompactEnabled === undefined
					? current.currentAutoCompactEnabled
					: controls.autoCompactEnabled;
			return {
				transcripts: {
					...state.transcripts,
					[key]: {
						...current,
						currentAgentId: currentAgentId ?? null,
						currentModelId: currentModelId ?? null,
						currentThinkingEffort: currentThinkingEffort ?? null,
						currentApprovalMode,
						currentAutoCompactEnabled,
						availableModels: ensureModelOption(
							current.availableModels,
							currentModelId ?? null,
							controls.rawModelId ?? currentModelId ?? null,
							controls.providerId ?? null,
							null,
							controls.providerProfileId ?? null,
						),
					},
				},
			};
		}),
	updateContextSnapshot: (key, update) =>
		set((state) => {
			const current = state.transcripts[key];
			if (!current) return state;
			return {
				transcripts: {
					...state.transcripts,
					[key]: {
						...current,
						contextSnapshot:
							update.snapshot ??
							(update.error ? current.contextSnapshot : null),
						contextSnapshotError: update.error,
						contextSnapshotRequestId: update.requestId,
						lastSeq:
							update.seq == null
								? current.lastSeq
								: Math.max(current.lastSeq, update.seq),
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

export function modelOptionId(
	providerProfileId: string | null | undefined,
	modelId: string | null | undefined,
): string | null {
	const trimmedModelId = modelId?.trim() ?? "";
	if (!trimmedModelId) return null;
	const trimmedProfileId = providerProfileId?.trim() ?? "";
	return trimmedProfileId
		? `${trimmedProfileId}:${trimmedModelId}`
		: trimmedModelId;
}

export function normalizeModelOptions(
	options: readonly RawSessionModelOption[],
): SessionModelOption[] {
	const normalized: SessionModelOption[] = [];
	for (const option of options) {
		if (typeof option.id !== "string") continue;
		const id = option.id.trim();
		if (!id) continue;
		const modelId =
			typeof option.modelId === "string" && option.modelId.trim()
				? option.modelId.trim()
				: id;
		const label =
			typeof option.label === "string" && option.label.trim()
				? option.label.trim()
				: modelId;
		const thinkingEffortOptions = Array.isArray(option.thinkingEffortOptions)
			? option.thinkingEffortOptions
					.map(parseThinkingEffort)
					.filter((value): value is SessionThinkingEffort => value !== null)
			: [];
		const defaultThinkingEffort = parseThinkingEffort(
			option.defaultThinkingEffort ?? null,
		);
		const thinkingSupported =
			option.thinkingSupported === true ||
			(option.thinkingSupported !== false && thinkingEffortOptions.length > 0);
		normalized.push({
			id,
			label,
			modelId,
			providerId:
				typeof option.providerId === "string" && option.providerId.trim()
					? option.providerId.trim()
					: null,
			providerLabel:
				typeof option.providerLabel === "string" && option.providerLabel.trim()
					? option.providerLabel.trim()
					: null,
			providerProfileId:
				typeof option.providerProfileId === "string" &&
				option.providerProfileId.trim()
					? option.providerProfileId.trim()
					: null,
			thinkingSupported,
			thinkingEffortOptions,
			defaultThinkingEffort,
			inputModalities: normalizeStringList(option.inputModalities),
			supportedTypes: normalizeStringList(option.supportedTypes),
			attachmentStatus:
				typeof option.attachmentStatus === "string" &&
				option.attachmentStatus.trim()
					? option.attachmentStatus.trim()
					: null,
		});
	}
	return normalized;
}

function normalizeStringList(
	values: ReadonlyArray<string | null> | null | undefined,
): string[] {
	if (!Array.isArray(values)) return [];
	return values
		.map((value) => (typeof value === "string" ? value.trim() : ""))
		.filter((value) => value.length > 0);
}

export function parseThinkingEffort(
	value: string | null | undefined,
): SessionThinkingEffort | null {
	if (typeof value !== "string") return null;
	const trimmed = value.trim().toLowerCase();
	switch (trimmed) {
		case "none":
		case "minimal":
		case "low":
		case "medium":
		case "high":
		case "x_high":
			return trimmed;
		case "xhigh":
			return "x_high";
		default:
			return null;
	}
}

export function parseApprovalMode(
	value: string | null | undefined,
): SessionApprovalMode | null {
	if (typeof value !== "string") return null;
	switch (value.trim().toLowerCase()) {
		case "suggest":
			return "suggest";
		case "auto_edit":
		case "auto-edit":
		case "autoedit":
			return "auto_edit";
		case "yolo":
			return "yolo";
		default:
			return null;
	}
}

function ensureModelOption(
	options: readonly SessionModelOption[],
	id: string | null,
	modelId: string | null,
	providerId: string | null,
	providerLabel: string | null,
	providerProfileId: string | null,
): SessionModelOption[] {
	if (!id || options.some((option) => option.id === id)) {
		return [...options];
	}
	const normalizedModelId = modelId?.trim() || id;
	return [
		{
			id,
			label: normalizedModelId,
			modelId: normalizedModelId,
			providerId,
			providerLabel,
			providerProfileId,
			thinkingSupported: false,
			thinkingEffortOptions: [],
			defaultThinkingEffort: null,
			inputModalities: [],
			supportedTypes: [],
			attachmentStatus: null,
		},
		...options,
	];
}
