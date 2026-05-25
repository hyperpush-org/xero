import { type Channel, Presence, type Socket } from "phoenix";
import { useEffect, useRef, useState } from "react";
import { applyRemoteThemeEnvelope } from "#/lib/theme/cloud-theme";
import type { AccountDevice } from "../auth/session";
import { decodeRelayFrame } from "./envelope";
import {
	getRelaySocket,
	joinSessionChannel,
	pushInboundCommand,
	requestSessionArchive,
	requestStartSession,
	requestThemeSnapshot,
} from "./relay-client";
import {
	GLOBAL_COMPUTER_USE_PROJECT_ID,
	modelOptionId,
	normalizeModelOptions,
	parseThinkingEffort,
	REMOTE_COMPUTER_USE_SESSION_ID,
	type RemoteProjectSummary,
	type SessionContextError,
	type SessionContextSnapshot,
	type SessionKind,
	type SessionThinkingEffort,
	type SessionTranscript,
	sessionKey,
	useSessionStore,
	type VisibleSessionSummary,
} from "./session-store";
import { projectRemotePayloadToTurns } from "./stream-projection";
import { remoteProjectsUpdateFromEnvelope } from "./visible-projects";
import {
	type RemoteVisibleSessionUpdate,
	remoteVisibleSessionUpdateFromEnvelope,
} from "./visible-sessions";

interface SessionSnapshotPayload {
	schema: string;
	projectId: string;
	session: {
		agentSessionId: string;
		agent_session_id?: string;
		title?: string | null;
		lastActivityAt?: string | null;
		updated_at?: string | null;
	};
	availableAgents?: { id: string; label: string }[];
	availableModels?: Array<{
		id?: string;
		label?: string;
		modelId?: string | null;
		providerId?: string | null;
		providerLabel?: string | null;
		providerProfileId?: string | null;
		thinkingSupported?: boolean | null;
		thinkingEffortOptions?: ReadonlyArray<string | null> | null;
		defaultThinkingEffort?: string | null;
	}>;
	transcript?: unknown[];
	runs?: unknown[];
	runtimeRun?: unknown;
	selectedControls?: unknown;
	contextSnapshot?: SessionContextSnapshot | null;
	contextSnapshotError?: SessionContextError | null;
}

interface RemoteControlSelection {
	agentId: string | null;
	modelId: string | null;
	rawModelId: string | null;
	providerId: string | null;
	providerProfileId: string | null;
	thinkingEffort: SessionThinkingEffort | null;
	autoCompactEnabled: boolean;
}

interface UseSessionStreamOptions {
	computerId: string;
	sessionId: string;
	relayToken: string;
	enabled?: boolean;
}

export interface AccountRemoteSessionsState {
	sessions: VisibleSessionSummary[];
	projects: RemoteProjectSummary[];
	startSession: (
		project: RemoteProjectSummary,
		options?: { sessionKind?: SessionKind },
	) => boolean;
	archiveSession: (summary: VisibleSessionSummary) => boolean;
}

const UNRECONCILED_REMOTE_LIST_RETRY_MS = 2_000;
const RECONCILED_REMOTE_LIST_REFRESH_MS = 15_000;

/**
 * Connects to a remote session channel and pushes decoded snapshot/event frames
 * into the Zustand session store. Returns the underlying channel so callers can
 * dispatch inbound commands.
 */
export function useSessionStream({
	computerId,
	enabled = true,
	sessionId,
	relayToken,
}: UseSessionStreamOptions): {
	channel: Channel | null;
	joinRejected: boolean;
} {
	const [channel, setChannel] = useState<Channel | null>(null);
	const [joinRejected, setJoinRejected] = useState(false);
	const replaceWithSnapshot = useSessionStore((s) => s.replaceWithSnapshot);
	const appendTurn = useSessionStore((s) => s.appendTurn);
	const updateControls = useSessionStore((s) => s.updateControls);
	const updateContextSnapshot = useSessionStore((s) => s.updateContextSnapshot);
	const removeVisibleSession = useSessionStore((s) => s.removeVisibleSession);
	const setLive = useSessionStore((s) => s.setLive);
	const relayTokenRef = useLatestRelayToken(relayToken);

	useEffect(() => {
		if (!enabled) {
			setChannel(null);
			setJoinRejected(false);
			return;
		}
		let disposed = false;
		const key = sessionKey(computerId, sessionId);
		const socket: Socket = getRelaySocket(relayTokenRef.current);
		const initialLastSeq =
			useSessionStore.getState().transcripts[key]?.lastSeq ?? 0;
		setJoinRejected(false);

		const sessionChannel = joinSessionChannel(
			socket,
			computerId,
			sessionId,
			initialLastSeq,
			(joinedChannel) => {
				if (!disposed) setChannel(joinedChannel);
			},
			() => {
				if (disposed) return;
				removeVisibleSession(computerId, sessionId);
				setLive(key, false);
				setJoinRejected(true);
				setChannel((current) => (current === sessionChannel ? null : current));
			},
		);

		sessionChannel.on("frame", (rawFrame: unknown) => {
			const envelope = decodeRelayFrame(rawFrame);
			if (!envelope) return;
			if (envelope.kind === "snapshot") {
				const snapshot = envelope.payload as SessionSnapshotPayload;
				const initialTurns = projectRemotePayloadToTurns(snapshot);
				const controls = remoteSnapshotControlSelection(snapshot);
				const availableModels = normalizeModelOptions(
					snapshot.availableModels ?? [],
				);
				const next: SessionTranscript = {
					turns: initialTurns,
					lastSeq: envelope.seq,
					isLive: remoteSnapshotIsLive(snapshot),
					availableAgents: snapshot.availableAgents ?? [],
					availableModels: ensureModelOption(
						availableModels,
						controls.modelId,
						controls.rawModelId,
						controls.providerId,
						null,
						controls.providerProfileId,
					),
					currentAgentId: controls.agentId,
					currentModelId: controls.modelId,
					currentThinkingEffort: controls.thinkingEffort,
					currentAutoCompactEnabled: controls.autoCompactEnabled,
					contextSnapshot: snapshot.contextSnapshot ?? null,
					contextSnapshotError: snapshot.contextSnapshotError ?? null,
					contextSnapshotRequestId: null,
				};
				replaceWithSnapshot(key, next);
			} else if (envelope.kind === "event") {
				const commandFailure = remoteCommandFailureTurn(
					envelope.payload,
					envelope.seq,
				);
				if (commandFailure) {
					const current = useSessionStore.getState().transcripts[key];
					if (current) {
						appendTurn(key, commandFailure, envelope.seq);
					} else {
						replaceWithSnapshot(key, {
							turns: [commandFailure],
							lastSeq: envelope.seq,
							isLive: false,
							availableAgents: [],
							availableModels: [],
							currentAgentId: null,
							currentModelId: null,
							currentThinkingEffort: null,
							currentAutoCompactEnabled: true,
							contextSnapshot: null,
							contextSnapshotError: null,
							contextSnapshotRequestId: null,
						});
					}
					setLive(key, false);
					return;
				}
				const turns = projectRemotePayloadToTurns(envelope.payload);
				for (const turn of turns) {
					appendTurn(key, turn, envelope.seq);
				}
				const contextUpdate = remoteContextSnapshotUpdate(envelope.payload);
				if (contextUpdate) {
					updateContextSnapshot(key, {
						...contextUpdate,
						seq: envelope.seq,
					});
				}
				const controls = remoteEventControlSelection(envelope.payload);
				if (controls) updateControls(key, controls);
				if (isTerminalRemoteEvent(envelope.payload)) {
					setLive(key, false);
				} else if (turns.length > 0 || controls) {
					setLive(key, true);
				}
			}
		});

		return () => {
			disposed = true;
			setLive(key, false);
			sessionChannel.leave();
			setChannel((current) => (current === sessionChannel ? null : current));
		};
	}, [
		appendTurn,
		computerId,
		enabled,
		replaceWithSnapshot,
		removeVisibleSession,
		relayTokenRef,
		sessionId,
		setLive,
		updateContextSnapshot,
		updateControls,
	]);

	return { channel, joinRejected };
}

/** Subscribe account presence and request visible sessions from online desktops. */
export function useAccountRemoteSessions(
	relayToken: string,
	accountId: string,
	devices: readonly AccountDevice[] = [],
	webDeviceId?: string | null,
): AccountRemoteSessionsState {
	const visibleSessions = useSessionStore((s) => s.visibleSessions);
	const remoteProjectsByComputer = useSessionStore(
		(s) => s.remoteProjectsByComputer,
	);
	const clearVisibleSessionsForComputers = useSessionStore(
		(s) => s.clearVisibleSessionsForComputers,
	);
	const replaceVisibleSessionsForComputer = useSessionStore(
		(s) => s.replaceVisibleSessionsForComputer,
	);
	const replaceRemoteProjectsForComputer = useSessionStore(
		(s) => s.replaceRemoteProjectsForComputer,
	);
	const clearRemoteProjectsForComputers = useSessionStore(
		(s) => s.clearRemoteProjectsForComputers,
	);
	const upsertVisibleSession = useSessionStore((s) => s.upsertVisibleSession);
	const removeVisibleSession = useSessionStore((s) => s.removeVisibleSession);
	const onlineComputerIds = useSessionStore((s) => s.onlineComputerIds);
	const setOnlineComputers = useSessionStore((s) => s.setOnlineComputers);
	const resetComputerPresence = useSessionStore((s) => s.resetComputerPresence);
	const relayTokenRef = useLatestRelayToken(relayToken);
	const sessionListChannelsRef = useRef(new Map<string, Channel>());
	const projectListChannelsRef = useRef(new Map<string, Channel>());
	const newSessionChannelsRef = useRef(new Map<string, Channel>());
	const themeChannelsRef = useRef(new Map<string, Channel>());

	useEffect(() => {
		if (!relayTokenRef.current || !accountId) return;
		let disposed = false;
		const socket = getRelaySocket(relayTokenRef.current);
		const channel = socket.channel(`account:${accountId}`, {});
		const presence = new Presence(channel);
		presence.onSync(() => {
			if (disposed) return;
			setOnlineComputers(
				presence
					.list<string | null>((id, entry) =>
						typeof id === "string" && hasDesktopPresence(entry) ? id : null,
					)
					.filter((id): id is string => typeof id === "string"),
			);
		});
		resetComputerPresence();
		channel.join();
		return () => {
			disposed = true;
			channel.leave();
			resetComputerPresence();
		};
	}, [accountId, relayTokenRef, resetComputerPresence, setOnlineComputers]);

	useEffect(() => {
		if (!relayTokenRef.current || !accountId) return;
		let disposed = false;
		const socket = getRelaySocket(relayTokenRef.current);
		const stopReconciliationTimers: Array<() => void> = [];
		const desktopDevices = devices.filter(
			(device) => device.kind === "desktop" && !device.revoked_at,
		);
		const onlineDesktopIds = Object.keys(onlineComputerIds);
		const clearComputerIds = new Set(desktopDevices.map((device) => device.id));
		for (const session of useSessionStore.getState().visibleSessions) {
			clearComputerIds.add(session.computerId);
		}
		for (const computerId of Object.keys(
			useSessionStore.getState().remoteProjectsByComputer,
		)) {
			clearComputerIds.add(computerId);
		}
		const offlineComputerIds = [...clearComputerIds].filter(
			(computerId) => !onlineComputerIds[computerId],
		);
		clearVisibleSessionsForComputers(offlineComputerIds);
		clearRemoteProjectsForComputers(offlineComputerIds);
		const applyUpdate = (update: RemoteVisibleSessionUpdate) => {
			if (update.kind === "replace") {
				replaceVisibleSessionsForComputer(update.computerId, update.sessions);
				return;
			}
			if (update.kind === "remove") {
				removeVisibleSession(update.computerId, update.sessionId);
				return;
			}
			upsertVisibleSession(update.summary);
		};

		const sessionListChannels = onlineDesktopIds.map((computerId) => {
			const sessionListChannel = joinSessionChannel(
				socket,
				computerId,
				"__sessions__",
				0,
				(joinedChannel) => {
					if (disposed || !webDeviceId) return;
					stopReconciliationTimers.push(
						scheduleRemoteListReconciliation({
							isDisposed: () => disposed,
							isReconciled: () =>
								Boolean(
									useSessionStore.getState().visibleSessionsByComputerVersion[
										computerId
									],
								),
							request: () =>
								requestVisibleSessions(joinedChannel, computerId, webDeviceId),
						}),
					);
				},
			);
			sessionListChannelsRef.current.set(computerId, sessionListChannel);
			sessionListChannel.on("frame", (rawFrame: unknown) => {
				const envelope = decodeRelayFrame(rawFrame);
				if (!envelope) return;
				const update = remoteVisibleSessionUpdateFromEnvelope(
					envelope,
					devices,
				);
				if (update) applyUpdate(update);
			});
			return sessionListChannel;
		});

		const projectListChannels = onlineDesktopIds.map((computerId) => {
			const projectListChannel = joinSessionChannel(
				socket,
				computerId,
				"__projects__",
				0,
				(joinedChannel) => {
					if (disposed || !webDeviceId) return;
					stopReconciliationTimers.push(
						scheduleRemoteListReconciliation({
							isDisposed: () => disposed,
							isReconciled: () =>
								Boolean(
									useSessionStore.getState().remoteProjectsByComputer[
										computerId
									],
								),
							request: () =>
								requestProjectList(joinedChannel, computerId, webDeviceId),
						}),
					);
				},
			);
			projectListChannelsRef.current.set(computerId, projectListChannel);
			projectListChannel.on("frame", (rawFrame: unknown) => {
				const envelope = decodeRelayFrame(rawFrame);
				if (!envelope) return;
				const update = remoteProjectsUpdateFromEnvelope(envelope);
				if (update) {
					replaceRemoteProjectsForComputer(update.computerId, update.projects);
				}
			});
			return projectListChannel;
		});

		const newSessionChannels = onlineDesktopIds.map((computerId) => {
			const newSessionChannel = joinSessionChannel(
				socket,
				computerId,
				"__new__",
				0,
			);
			newSessionChannelsRef.current.set(computerId, newSessionChannel);
			return newSessionChannel;
		});

		const themeChannels = onlineDesktopIds.map((computerId) => {
			const themeChannel = joinSessionChannel(
				socket,
				computerId,
				"__theme__",
				0,
				(joinedChannel) => {
					if (disposed || !webDeviceId) return;
					requestThemeSnapshot(joinedChannel, {
						computerId,
						deviceId: webDeviceId,
					});
				},
			);
			themeChannelsRef.current.set(computerId, themeChannel);
			themeChannel.on("frame", (rawFrame: unknown) => {
				const envelope = decodeRelayFrame(rawFrame);
				if (!envelope) return;
				applyRemoteThemeEnvelope(envelope);
			});
			return themeChannel;
		});

		return () => {
			disposed = true;
			for (const stopTimer of stopReconciliationTimers) {
				stopTimer();
			}
			for (const sessionListChannel of sessionListChannels) {
				sessionListChannel.leave();
			}
			for (const projectListChannel of projectListChannels) {
				projectListChannel.leave();
			}
			for (const newSessionChannel of newSessionChannels) {
				newSessionChannel.leave();
			}
			for (const themeChannel of themeChannels) {
				themeChannel.leave();
			}
			for (const [index, computerId] of onlineDesktopIds.entries()) {
				if (
					sessionListChannelsRef.current.get(computerId) ===
					sessionListChannels[index]
				) {
					sessionListChannelsRef.current.delete(computerId);
				}
				if (
					projectListChannelsRef.current.get(computerId) ===
					projectListChannels[index]
				) {
					projectListChannelsRef.current.delete(computerId);
				}
				if (
					newSessionChannelsRef.current.get(computerId) ===
					newSessionChannels[index]
				) {
					newSessionChannelsRef.current.delete(computerId);
				}
				if (themeChannelsRef.current.get(computerId) === themeChannels[index]) {
					themeChannelsRef.current.delete(computerId);
				}
			}
		};
	}, [
		accountId,
		clearRemoteProjectsForComputers,
		clearVisibleSessionsForComputers,
		devices,
		onlineComputerIds,
		relayTokenRef,
		removeVisibleSession,
		replaceRemoteProjectsForComputer,
		replaceVisibleSessionsForComputer,
		upsertVisibleSession,
		webDeviceId,
	]);

	const startSession = (
		project: RemoteProjectSummary,
		options: { sessionKind?: SessionKind } = {},
	): boolean => {
		if (!webDeviceId) return false;
		const channel = newSessionChannelsRef.current.get(project.computerId);
		if (!channel) return false;
		const sessionKind = options.sessionKind ?? "standard";
		requestStartSession(channel, {
			computerId: project.computerId,
			projectId: project.projectId,
			deviceId: webDeviceId,
			sessionKind,
			agent: sessionKind === "computer_use" ? "computer_use" : null,
		});
		return true;
	};

	const archiveSession = (summary: VisibleSessionSummary): boolean => {
		if (!webDeviceId) return false;
		const channel = sessionListChannelsRef.current.get(summary.computerId);
		if (!channel) return false;
		requestSessionArchive(channel, {
			computerId: summary.computerId,
			projectId: summary.projectId,
			sessionId: summary.sessionId,
			agentSessionId: summary.agentSessionId,
			deviceId: webDeviceId,
		});
		return true;
	};

	const projects = flattenRemoteProjects(remoteProjectsByComputer);
	const sessions = withGlobalComputerUseSessions(
		visibleSessions,
		onlineComputerIds,
		devices,
	);

	return {
		sessions,
		projects,
		startSession,
		archiveSession,
	};
}

export function withGlobalComputerUseSessions(
	visibleSessions: readonly VisibleSessionSummary[],
	onlineComputerIds: Record<string, true>,
	devices: readonly AccountDevice[],
): VisibleSessionSummary[] {
	const existingKeys = new Set(
		visibleSessions.map((session) =>
			sessionKey(session.computerId, session.sessionId),
		),
	);
	const next: VisibleSessionSummary[] = [...visibleSessions];
	for (const computerId of Object.keys(onlineComputerIds).sort()) {
		const key = sessionKey(computerId, REMOTE_COMPUTER_USE_SESSION_ID);
		if (existingKeys.has(key)) continue;
		const computerName =
			devices.find((device) => device.id === computerId)?.name ?? null;
		next.push({
			computerId,
			sessionId: REMOTE_COMPUTER_USE_SESSION_ID,
			agentSessionId: REMOTE_COMPUTER_USE_SESSION_ID,
			projectId: GLOBAL_COMPUTER_USE_PROJECT_ID,
			projectName: null,
			sessionKind: "computer_use",
			isComputerUse: true,
			title: "Computer Use",
			lastActivityAt: null,
			computerName,
			remoteVisible: true,
		});
	}
	return next;
}

export function useAccountVisibleSessions(
	relayToken: string,
	accountId: string,
	devices: readonly AccountDevice[] = [],
	webDeviceId?: string | null,
): VisibleSessionSummary[] {
	return useAccountRemoteSessions(relayToken, accountId, devices, webDeviceId)
		.sessions;
}

function useLatestRelayToken(relayToken: string) {
	const relayTokenRef = useRef(relayToken);
	useEffect(() => {
		relayTokenRef.current = relayToken;
	}, [relayToken]);
	return relayTokenRef;
}

function hasDesktopPresence(entry: unknown): boolean {
	if (!isRecord(entry) || !Array.isArray(entry.metas)) return false;
	return entry.metas.some((meta) => isRecord(meta) && meta.kind === "desktop");
}

function scheduleRemoteListReconciliation(options: {
	isDisposed: () => boolean;
	isReconciled: () => boolean;
	request: () => void;
}): () => void {
	let timeout: ReturnType<typeof setTimeout> | null = null;
	const tick = () => {
		if (options.isDisposed()) return;
		options.request();
		timeout = setTimeout(
			tick,
			options.isReconciled()
				? RECONCILED_REMOTE_LIST_REFRESH_MS
				: UNRECONCILED_REMOTE_LIST_RETRY_MS,
		);
	};
	tick();
	return () => {
		if (timeout) clearTimeout(timeout);
	};
}

function requestVisibleSessions(
	channel: Channel,
	computerId: string,
	webDeviceId: string,
): void {
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: computerId,
		session_id: "__sessions__",
		device_id: webDeviceId,
		kind: "list_sessions",
		payload: {},
	});
}

function requestProjectList(
	channel: Channel,
	computerId: string,
	webDeviceId: string,
): void {
	pushInboundCommand(channel, {
		v: 1,
		seq: Date.now(),
		computer_id: computerId,
		session_id: "__projects__",
		device_id: webDeviceId,
		kind: "list_projects",
		payload: {},
	});
}

function remoteContextSnapshotUpdate(payload: unknown): {
	snapshot: SessionContextSnapshot | null;
	error: SessionContextError | null;
	requestId: string | null;
} | null {
	if (!isRecord(payload)) return null;
	if (payload.schema !== "xero.remote_context_snapshot.v1") return null;
	const requestId = stringField(payload, "requestId");
	const ok = payload.ok !== false;
	return {
		snapshot: ok ? contextSnapshotField(payload, "contextSnapshot") : null,
		error: ok ? null : contextErrorField(payload, "error"),
		requestId,
	};
}

function remoteCommandFailureTurn(
	payload: unknown,
	sequence: number,
): SessionTranscript["turns"][number] | null {
	if (!isRecord(payload)) return null;
	if (payload.schema !== "xero.remote_command_result.v1") return null;
	if (payload.ok !== false) return null;
	const error = recordField(payload, "error");
	const code = stringField(error, "code") ?? "remote_command_failed";
	const message =
		stringField(error, "message") ?? "Xero could not load this remote session.";
	return {
		id: `failure:remote-command:${sequence}`,
		kind: "failure",
		sequence,
		code,
		message,
	};
}

function flattenRemoteProjects(
	projectsByComputer: Record<string, RemoteProjectSummary[]>,
): RemoteProjectSummary[] {
	return Object.values(projectsByComputer).flat();
}

function remoteEventControlSelection(
	payload: unknown,
): RemoteControlSelection | null {
	if (!isRecord(payload)) return null;
	const schema = stringField(payload, "schema");
	if (
		schema !== "xero.remote_message_accepted.v1" &&
		schema !== "xero.remote_session_started.v1" &&
		schema !== "xero.remote_session_controls_updated.v1"
	) {
		return null;
	}
	const result = recordField(payload, "result");
	const controls =
		recordField(result, "controls") ?? recordField(payload, "controls");
	const selectedControls = remoteControlSelectionFromRecord(controls, null);
	if (selectedControls) return selectedControls;
	const run = recordField(result, "run");
	return run ? remoteRunControlSelection(run) : null;
}

export function remoteSnapshotControlSelection(
	snapshot: SessionSnapshotPayload,
): RemoteControlSelection {
	const run = isRecord(snapshot.runtimeRun) ? snapshot.runtimeRun : null;
	if (runtimeRunCanSupplySelectedControls(run)) {
		return remoteRunControlSelection(run);
	}
	const selectedControls = remoteControlSelectionFromRecord(
		recordField(
			snapshot as unknown as Record<string, unknown>,
			"selectedControls",
		),
		null,
	);
	if (selectedControls) return selectedControls;
	return remoteRunControlSelection(run);
}

function isTerminalRemoteEvent(payload: unknown): boolean {
	if (!isRecord(payload)) return false;
	if (payload.schema === "xero.remote_runtime_event.v1") {
		const eventKind = stringField(payload, "eventKind");
		return eventKind === "run_completed" || eventKind === "run_failed";
	}
	const kind = stringField(payload, "kind");
	return kind === "complete" || kind === "failure";
}

function remoteSnapshotIsLive(snapshot: SessionSnapshotPayload): boolean {
	const runtimeRun = isRecord(snapshot.runtimeRun) ? snapshot.runtimeRun : null;
	const runtimeStatus = stringField(runtimeRun, "status");
	if (runtimeStatus) return isLiveRuntimeStatus(runtimeStatus);

	const runs = Array.isArray(snapshot.runs)
		? snapshot.runs.filter(isRecord)
		: [];
	const latestRun = runs.at(-1) ?? null;
	const runStatus = stringField(latestRun, "status");
	return runStatus ? isLiveAgentRunStatus(runStatus) : false;
}

function isLiveRuntimeStatus(status: string): boolean {
	return status === "starting" || status === "running";
}

function isLiveAgentRunStatus(status: string): boolean {
	return (
		status === "starting" ||
		status === "running" ||
		status === "paused" ||
		status === "cancelling"
	);
}

function remoteRunControlSelection(run: unknown): RemoteControlSelection {
	if (!isRecord(run)) {
		return {
			agentId: null,
			modelId: null,
			rawModelId: null,
			providerId: null,
			providerProfileId: null,
			thinkingEffort: null,
			autoCompactEnabled: true,
		};
	}
	const controls = recordField(run, "controls");
	const selected =
		recordField(controls, "pending") ?? recordField(controls, "active");
	return (
		remoteControlSelectionFromRecord(selected, {
			providerId: stringField(run, "providerId"),
		}) ?? {
			agentId: null,
			modelId: null,
			rawModelId: null,
			providerId: null,
			providerProfileId: null,
			thinkingEffort: null,
			autoCompactEnabled: true,
		}
	);
}

function runtimeRunCanSupplySelectedControls(
	run: Record<string, unknown> | null,
): boolean {
	if (!run) return false;
	const isTerminal = booleanField(run, "isTerminal", "is_terminal");
	if (isTerminal === true) return false;
	const status = stringField(run, "status");
	return status !== "stopped" && status !== "failed";
}

function remoteControlSelectionFromRecord(
	selected: Record<string, unknown> | null,
	fallback: { providerId?: string | null } | null,
): RemoteControlSelection | null {
	if (!selected) return null;
	const rawModelId = stringField(selected, "modelId", "model_id");
	const providerProfileId = stringField(
		selected,
		"providerProfileId",
		"provider_profile_id",
	);
	const autoCompactEnabledRaw =
		selected.autoCompactEnabled ?? selected.auto_compact_enabled;
	return {
		agentId: stringField(
			selected,
			"runtimeAgentId",
			"runtime_agent_id",
			"agent",
		),
		modelId: modelOptionId(providerProfileId, rawModelId),
		rawModelId,
		providerId:
			stringField(selected, "providerId", "provider_id") ??
			fallback?.providerId ??
			null,
		providerProfileId,
		thinkingEffort: parseThinkingEffort(
			stringField(selected, "thinkingEffort", "thinking_effort"),
		),
		autoCompactEnabled:
			typeof autoCompactEnabledRaw === "boolean" ? autoCompactEnabledRaw : true,
	};
}

function ensureModelOption(
	options: ReturnType<typeof normalizeModelOptions>,
	id: string | null,
	modelId: string | null,
	providerId: string | null,
	providerLabel: string | null,
	providerProfileId: string | null,
) {
	if (!id || options.some((option) => option.id === id)) return [...options];
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
		},
		...options,
	];
}

function recordField(
	record: Record<string, unknown> | null | undefined,
	key: string,
): Record<string, unknown> | null {
	if (!record) return null;
	const value = record[key];
	return isRecord(value) ? value : null;
}

function contextSnapshotField(
	record: Record<string, unknown>,
	key: string,
): SessionContextSnapshot | null {
	const value = record[key];
	return isRecord(value) ? (value as SessionContextSnapshot) : null;
}

function contextErrorField(
	record: Record<string, unknown>,
	key: string,
): SessionContextError | null {
	const value = record[key];
	return isRecord(value) ? (value as SessionContextError) : null;
}

function stringField(
	record: Record<string, unknown> | null | undefined,
	...keys: string[]
): string | null {
	if (!record) return null;
	for (const key of keys) {
		const value = record[key];
		if (typeof value === "string" && value.trim().length > 0) {
			return value.trim();
		}
	}
	return null;
}

function booleanField(
	record: Record<string, unknown> | null | undefined,
	...keys: string[]
): boolean | null {
	if (!record) return null;
	for (const key of keys) {
		const value = record[key];
		if (typeof value === "boolean") return value;
	}
	return null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return Boolean(value && typeof value === "object" && !Array.isArray(value));
}

export type { AccountDevice };
