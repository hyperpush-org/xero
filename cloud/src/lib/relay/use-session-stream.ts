import { type Channel, Presence, type Socket } from "phoenix";
import { useEffect, useRef, useState } from "react";
import type { AccountDevice } from "../auth/session";
import { decodeRelayFrame } from "./envelope";
import {
	getRelaySocket,
	joinSessionChannel,
	pushInboundCommand,
	requestSessionArchive,
	requestSessionRemoteVisibility,
} from "./relay-client";
import {
	modelOptionId,
	normalizeModelOptions,
	type RemoteProjectSummary,
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
		providerProfileId?: string | null;
	}>;
	transcript?: unknown[];
	runs?: unknown[];
	runtimeRun?: unknown;
}

interface RemoteControlSelection {
	agentId: string | null;
	modelId: string | null;
	rawModelId: string | null;
	providerId: string | null;
	providerProfileId: string | null;
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
	setSessionRemoteVisibility: (
		summary: VisibleSessionSummary,
		visible: boolean,
	) => boolean;
	archiveSession: (summary: VisibleSessionSummary) => boolean;
}

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
				const controls = remoteRunControlSelection(snapshot.runtimeRun);
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
						controls.providerProfileId,
					),
					currentAgentId: controls.agentId,
					currentModelId: controls.modelId,
				};
				replaceWithSnapshot(key, next);
			} else if (envelope.kind === "event") {
				const turns = projectRemotePayloadToTurns(envelope.payload);
				for (const turn of turns) {
					appendTurn(key, turn, envelope.seq);
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
		const retryHandles: ReturnType<typeof setInterval>[] = [];
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
					requestVisibleSessions(joinedChannel, computerId, webDeviceId);
					const retryHandle = setInterval(() => {
						if (disposed) {
							clearInterval(retryHandle);
							return;
						}
						const reconciled =
							useSessionStore.getState().visibleSessionsByComputerVersion[
								computerId
							];
						if (reconciled) {
							clearInterval(retryHandle);
							return;
						}
						requestVisibleSessions(joinedChannel, computerId, webDeviceId);
					}, 2_000);
					retryHandles.push(retryHandle);
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
					requestProjectList(joinedChannel, computerId, webDeviceId);
					const retryHandle = setInterval(() => {
						if (disposed) {
							clearInterval(retryHandle);
							return;
						}
						const reconciled =
							useSessionStore.getState().remoteProjectsByComputer[computerId];
						if (reconciled) {
							clearInterval(retryHandle);
							return;
						}
						requestProjectList(joinedChannel, computerId, webDeviceId);
					}, 2_000);
					retryHandles.push(retryHandle);
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

		return () => {
			disposed = true;
			for (const retryHandle of retryHandles) {
				clearInterval(retryHandle);
			}
			for (const sessionListChannel of sessionListChannels) {
				sessionListChannel.leave();
			}
			for (const projectListChannel of projectListChannels) {
				projectListChannel.leave();
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

	const setSessionRemoteVisibility = (
		summary: VisibleSessionSummary,
		visible: boolean,
	): boolean => {
		if (!webDeviceId) return false;
		const channel = sessionListChannelsRef.current.get(summary.computerId);
		if (!channel) return false;
		requestSessionRemoteVisibility(channel, {
			computerId: summary.computerId,
			projectId: summary.projectId,
			sessionId: summary.sessionId,
			deviceId: webDeviceId,
			visible,
		});
		upsertVisibleSession({ ...summary, remoteVisible: visible });
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
			deviceId: webDeviceId,
		});
		return true;
	};

	const projects = flattenRemoteProjects(remoteProjectsByComputer);

	return {
		sessions: visibleSessions,
		projects,
		setSessionRemoteVisibility,
		archiveSession,
	};
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
		schema !== "xero.remote_session_started.v1"
	) {
		return null;
	}
	const result = recordField(payload, "result");
	const run = recordField(result, "run");
	return run ? remoteRunControlSelection(run) : null;
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
		};
	}
	const controls = recordField(run, "controls");
	const selected =
		recordField(controls, "pending") ?? recordField(controls, "active");
	const rawModelId = stringField(selected, "modelId");
	const providerProfileId = stringField(selected, "providerProfileId");
	return {
		agentId: stringField(selected, "runtimeAgentId"),
		modelId: modelOptionId(providerProfileId, rawModelId),
		rawModelId,
		providerId: stringField(run, "providerId"),
		providerProfileId,
	};
}

function ensureModelOption(
	options: ReturnType<typeof normalizeModelOptions>,
	id: string | null,
	modelId: string | null,
	providerId: string | null,
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
			providerProfileId,
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

function stringField(
	record: Record<string, unknown> | null | undefined,
	key: string,
): string | null {
	if (!record) return null;
	const value = record[key];
	return typeof value === "string" && value.trim().length > 0
		? value.trim()
		: null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return Boolean(value && typeof value === "object" && !Array.isArray(value));
}

export type { AccountDevice };
