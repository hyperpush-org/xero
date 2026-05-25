import { createContext, useContext } from "react";

import type { CloudSession } from "#/lib/auth/session";
import type {
	RemoteProjectSummary,
	SessionKind,
	VisibleSessionSummary,
} from "#/lib/relay/session-store";

export interface ActiveSessionTarget {
	computerId: string;
	sessionId: string;
}

export interface SessionsShellContextValue {
	session: CloudSession;
	visibleSessions: VisibleSessionSummary[];
	projects: RemoteProjectSummary[];
	activeTarget: ActiveSessionTarget | null;
	activeSession: VisibleSessionSummary | null;
	activeSessionKey: string | null;
	activeProjectLabel: string;
	activeTargetValid: boolean;
	computerPresenceKnown: boolean;
	currentComputerOnline: boolean;
	currentComputerReconciled: boolean;
	visibleSessionsVersion: number;
	selectSession: (computerId: string, sessionId: string) => void;
	startSession: (
		project: RemoteProjectSummary,
		options?: { sessionKind?: SessionKind },
	) => void;
	archiveSession: (summary: VisibleSessionSummary) => boolean;
	reportActiveTargetInvalid: (targetKey: string) => void;
	pendingProjectKey: string | null;
}

export const SessionsShellContext =
	createContext<SessionsShellContextValue | null>(null);

export function useSessionsShell() {
	const context = useContext(SessionsShellContext);
	if (!context) {
		throw new Error("useSessionsShell must be used inside SessionsShell");
	}
	return context;
}
