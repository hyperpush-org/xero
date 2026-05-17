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
}: UseSessionStreamOptions): { channel: Channel | null } {
	const [channel, setChannel] = useState<Channel | null>(null);
	const setVisibleSessions = useSessionStore((s) => s.setVisibleSessions);
	const replaceWithSnapshot = useSessionStore((s) => s.replaceWithSnapshot);
	const appendTurn = useSessionStore((s) => s.appendTurn);
	const setLive = useSessionStore((s) => s.setLive);

	useEffect(() => {
		const key = sessionKey(computerId, sessionId);
		const socket: Socket = getRelaySocket(relayToken);

		const accountChannel = joinAccountChannel(socket, accountId);
		accountChannel.on(
			"visible_sessions",
			(payload: { sessions: VisibleSessionSummary[] }) => {
				setVisibleSessions(payload.sessions ?? []);
			},
		);

		const sessionChannel = joinSessionChannel(socket, computerId, sessionId);
		setChannel(sessionChannel);

		sessionChannel.on("frame", (rawFrame: unknown) => {
			const envelope = decodeRelayFrame(rawFrame);
			if (!envelope) return;
			if (envelope.kind === "snapshot") {
				const snapshot = envelope.payload as SessionSnapshotPayload;
				const initialTurns = projectRemotePayloadToTurns(snapshot);
				const next: SessionTranscript = {
					turns: initialTurns,
					lastSeq: envelope.seq,
					isLive: true,
					availableAgents: snapshot.availableAgents ?? [],
					availableModels: snapshot.availableModels ?? [],
				};
				replaceWithSnapshot(key, next);
			} else if (envelope.kind === "event") {
				const turns = projectRemotePayloadToTurns(envelope.payload);
				for (const turn of turns) {
					appendTurn(key, turn, envelope.seq);
				}
			}
		});

		return () => {
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
		sessionId,
		setLive,
		setVisibleSessions,
	]);

	return { channel };
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

	useEffect(() => {
		if (!relayToken || !accountId) return;
		const socket = getRelaySocket(relayToken);
		const visibleByComputer = new Map<string, VisibleSessionSummary[]>();
		const commitVisibleSessions = () => {
			setVisibleSessions(
				Array.from(visibleByComputer.values())
					.flat()
					.sort(compareVisibleSessions),
			);
		};
		const applyUpdate = (update: RemoteVisibleSessionUpdate) => {
			if (update.kind === "replace") {
				visibleByComputer.set(update.computerId, update.sessions);
				commitVisibleSessions();
				return;
			}
			const current = visibleByComputer.get(update.summary.computerId) ?? [];
			visibleByComputer.set(update.summary.computerId, [
				update.summary,
				...current.filter(
					(session) => session.sessionId !== update.summary.sessionId,
				),
			]);
			commitVisibleSessions();
		};

		const channel = joinAccountChannel(socket, accountId);
		channel.on(
			"visible_sessions",
			(payload: { sessions: VisibleSessionSummary[] }) => {
				setVisibleSessions(payload.sessions ?? []);
			},
		);
		const sessionListChannels = devices
			.filter((device) => device.kind === "desktop" && !device.revoked_at)
			.map((device) => {
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
	}, [accountId, devices, relayToken, setVisibleSessions, webDeviceId]);

	return visibleSessions;
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

export type { AccountDevice };
