import type { ConversationTurn } from "@xero/ui/components/transcript/conversation-section";
import { create } from "zustand";

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
}

interface SessionStoreState {
	visibleSessions: VisibleSessionSummary[];
	transcripts: Record<string, SessionTranscript>;
	setVisibleSessions: (sessions: VisibleSessionSummary[]) => void;
	replaceWithSnapshot: (key: string, transcript: SessionTranscript) => void;
	appendTurn: (key: string, turn: ConversationTurn, seq: number) => void;
	setLive: (key: string, isLive: boolean) => void;
}

export const sessionKey = (computerId: string, sessionId: string) =>
	`${computerId}:${sessionId}`;

export const useSessionStore = create<SessionStoreState>((set) => ({
	visibleSessions: [],
	transcripts: {},
	setVisibleSessions: (sessions) => set({ visibleSessions: sessions }),
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
						turns: [...current.turns, turn],
						lastSeq: Math.max(current.lastSeq, seq),
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
