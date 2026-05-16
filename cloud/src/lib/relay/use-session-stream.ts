import type { Channel, Socket } from "phoenix";
import { useEffect, useRef } from "react";
import type { AccountDevice } from "../auth/session";
import { decodeRelayFrame } from "./envelope";
import {
	getRelaySocket,
	joinAccountChannel,
	joinSessionChannel,
} from "./relay-client";
import {
	type SessionTranscript,
	sessionKey,
	useSessionStore,
	type VisibleSessionSummary,
} from "./session-store";
import { projectStreamItemsToTurns } from "./stream-projection";

interface SessionSnapshotPayload {
	schema: string;
	projectId: string;
	session: {
		agentSessionId: string;
		title?: string | null;
		lastActivityAt?: string | null;
	};
	availableAgents?: { id: string; label: string }[];
	availableModels?: { id: string; label: string }[];
	transcript?: unknown[];
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
	const channelRef = useRef<Channel | null>(null);
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
		channelRef.current = sessionChannel;

		sessionChannel.on("frame", (rawFrame: unknown) => {
			const envelope = decodeRelayFrame(rawFrame);
			if (!envelope) return;
			if (envelope.kind === "snapshot") {
				const snapshot = envelope.payload as SessionSnapshotPayload;
				const initialItems = Array.isArray(snapshot.transcript)
					? snapshot.transcript
					: [];
				const initialTurns = projectStreamItemsToTurns(initialItems as never[]);
				const next: SessionTranscript = {
					turns: initialTurns,
					lastSeq: envelope.seq,
					isLive: true,
					availableAgents: snapshot.availableAgents ?? [],
					availableModels: snapshot.availableModels ?? [],
				};
				replaceWithSnapshot(key, next);
			} else if (envelope.kind === "event") {
				const items = Array.isArray(envelope.payload)
					? envelope.payload
					: [envelope.payload];
				const turns = projectStreamItemsToTurns(items as never[]);
				for (const turn of turns) {
					appendTurn(key, turn, envelope.seq);
				}
			}
		});

		return () => {
			setLive(key, false);
			sessionChannel.leave();
			accountChannel.leave();
			channelRef.current = null;
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

	return { channel: channelRef.current };
}

/** Subscribe an account channel just for the visible-sessions list (used by the drawer). */
export function useAccountVisibleSessions(
	relayToken: string,
	accountId: string,
): VisibleSessionSummary[] {
	const visibleSessions = useSessionStore((s) => s.visibleSessions);
	const setVisibleSessions = useSessionStore((s) => s.setVisibleSessions);

	useEffect(() => {
		if (!relayToken || !accountId) return;
		const socket = getRelaySocket(relayToken);
		const channel = joinAccountChannel(socket, accountId);
		channel.on(
			"visible_sessions",
			(payload: { sessions: VisibleSessionSummary[] }) => {
				setVisibleSessions(payload.sessions ?? []);
			},
		);
		return () => {
			channel.leave();
		};
	}, [accountId, relayToken, setVisibleSessions]);

	return visibleSessions;
}

export type { AccountDevice };
