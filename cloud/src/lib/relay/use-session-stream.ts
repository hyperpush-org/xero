import type { Channel, Socket } from "phoenix";
import { useEffect, useState } from "react";
import type { AccountDevice } from "../auth/session";
import { decodeRelayFrame } from "./envelope";
import {
	getRelaySocket,
	joinAccountChannel,
	joinSessionChannel,
	pushInboundCommand,
} from "./relay-client";
import {
	type SessionTranscript,
	sessionKey,
	useSessionStore,
	type VisibleSessionSummary,
} from "./session-store";
import { projectRemotePayloadToTurns } from "./stream-projection";
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
	availableModels?: { id: string; label: string }[];
	transcript?: unknown[];
	runs?: unknown[];
	runtimeRun?: unknown;
}

interface RemoteControlSelection {
	agentId: string | null;
	modelId: string | null;
}

interface UseSessionStreamOptions {
	computerId: string;
	sessionId: string;
	relayToken: string;
	accountId: string;
}

/**
 * Connects to the relay, joins the account + session channels, and pushes
 * decoded snapshot/event frames into the Zustand session store. Returns the
 * underlying channel so callers can dispatch inbound commands.
 */
export function useSessionStream({
	computerId,
	sessionId,
	relayToken,
	accountId,
}: UseSessionStreamOptions): {
	channel: Channel | null;
	joinRejected: boolean;
} {
	const [channel, setChannel] = useState<Channel | null>(null);
	const [joinRejected, setJoinRejected] = useState(false);
	const setVisibleSessions = useSessionStore((s) => s.setVisibleSessions);
	const replaceWithSnapshot = useSessionStore((s) => s.replaceWithSnapshot);
	const appendTurn = useSessionStore((s) => s.appendTurn);
	const updateControls = useSessionStore((s) => s.updateControls);
	const removeVisibleSession = useSessionStore((s) => s.removeVisibleSession);
	const setLive = useSessionStore((s) => s.setLive);

	useEffect(() => {
		let disposed = false;
		const key = sessionKey(computerId, sessionId);
		const socket: Socket = getRelaySocket(relayToken);
		setJoinRejected(false);

		const accountChannel = joinAccountChannel(socket, accountId);
		accountChannel.on(
			"visible_sessions",
			(payload: { sessions: VisibleSessionSummary[] }) => {
				setVisibleSessions(payload.sessions ?? []);
			},
		);

		const sessionChannel = joinSessionChannel(
			socket,
			computerId,
			sessionId,
			0,
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
				const next: SessionTranscript = {
					turns: initialTurns,
					lastSeq: envelope.seq,
					isLive: true,
					availableAgents: snapshot.availableAgents ?? [],
					availableModels: ensureOption(
						snapshot.availableModels ?? [],
						controls.modelId,
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
			}
		});

		return () => {
			disposed = true;
			setLive(key, false);
			sessionChannel.leave();
			accountChannel.leave();
			setChannel((current) => (current === sessionChannel ? null : current));
		};
	}, [
		accountId,
		appendTurn,
		computerId,
		relayToken,
		replaceWithSnapshot,
		removeVisibleSession,
		sessionId,
		setLive,
		setVisibleSessions,
		updateControls,
	]);

	return { channel, joinRejected };
}

/** Subscribe an account channel just for the visible-sessions list (used by the drawer). */
export function useAccountVisibleSessions(
	relayToken: string,
	accountId: string,
	devices: readonly AccountDevice[] = [],
	webDeviceId?: string | null,
): VisibleSessionSummary[] {
	const visibleSessions = useSessionStore((s) => s.visibleSessions);
	const setVisibleSessions = useSessionStore((s) => s.setVisibleSessions);
	const clearVisibleSessionsForComputers = useSessionStore(
		(s) => s.clearVisibleSessionsForComputers,
	);
	const replaceVisibleSessionsForComputer = useSessionStore(
		(s) => s.replaceVisibleSessionsForComputer,
	);
	const upsertVisibleSession = useSessionStore((s) => s.upsertVisibleSession);

	useEffect(() => {
		if (!relayToken || !accountId) return;
		const socket = getRelaySocket(relayToken);
		const desktopDevices = devices.filter(
			(device) => device.kind === "desktop" && !device.revoked_at,
		);
		clearVisibleSessionsForComputers(desktopDevices.map((device) => device.id));
		const applyUpdate = (update: RemoteVisibleSessionUpdate) => {
			if (update.kind === "replace") {
				replaceVisibleSessionsForComputer(update.computerId, update.sessions);
				return;
			}
			upsertVisibleSession(update.summary);
		};

		const channel = joinAccountChannel(socket, accountId);
		channel.on(
			"visible_sessions",
			(payload: { sessions: VisibleSessionSummary[] }) => {
				setVisibleSessions(payload.sessions ?? []);
			},
		);
		const sessionListChannels = desktopDevices.map((device) => {
			const sessionListChannel = joinSessionChannel(
				socket,
				device.id,
				"__sessions__",
				0,
				(joinedChannel) => {
					if (!webDeviceId) return;
					pushInboundCommand(joinedChannel, {
						v: 1,
						seq: Date.now(),
						computer_id: device.id,
						session_id: "__sessions__",
						device_id: webDeviceId,
						kind: "list_sessions",
						payload: {},
					});
				},
			);
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
		return () => {
			channel.leave();
			for (const sessionListChannel of sessionListChannels) {
				sessionListChannel.leave();
			}
		};
	}, [
		accountId,
		clearVisibleSessionsForComputers,
		devices,
		relayToken,
		replaceVisibleSessionsForComputer,
		setVisibleSessions,
		upsertVisibleSession,
		webDeviceId,
	]);

	return visibleSessions;
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

function remoteRunControlSelection(run: unknown): RemoteControlSelection {
	if (!isRecord(run)) return { agentId: null, modelId: null };
	const controls = recordField(run, "controls");
	const selected =
		recordField(controls, "pending") ?? recordField(controls, "active");
	return {
		agentId: stringField(selected, "runtimeAgentId"),
		modelId: stringField(selected, "modelId"),
	};
}

function ensureOption(
	options: readonly { id: string; label: string }[],
	id: string | null,
): { id: string; label: string }[] {
	if (!id || options.some((option) => option.id === id)) return [...options];
	return [{ id, label: id }, ...options];
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
