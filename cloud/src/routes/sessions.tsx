import {
	createFileRoute,
	Outlet,
	redirect,
	useNavigate,
	useRouterState,
} from "@tanstack/react-router";
import { Button } from "@xero/ui/components/ui/button";
import {
	Empty,
	EmptyContent,
	EmptyDescription,
	EmptyHeader,
	EmptyMedia,
	EmptyTitle,
} from "@xero/ui/components/ui/empty";
import { Skeleton } from "@xero/ui/components/ui/skeleton";
import { Menu } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { BrandLogo } from "#/components/brand-logo";
import { SessionDrawer } from "#/components/session-drawer";
import { SessionListRow } from "#/components/session-list-row";
import { SessionSidebar } from "#/components/session-sidebar";
import { SessionTopBar } from "#/components/session-top-bar";
import {
	type CloudSession,
	getCachedCurrentSession,
	signOut,
} from "#/lib/auth/session";
import {
	type RemoteProjectSummary,
	type SessionKind,
	sessionKey,
	useSessionStore,
	type VisibleSessionSummary,
} from "#/lib/relay/session-store";
import {
	type ActiveSessionTarget,
	SessionsShellContext,
	type SessionsShellContextValue,
	useSessionsShell,
} from "#/lib/relay/sessions-shell-context";
import { useAccountRemoteSessions } from "#/lib/relay/use-session-stream";
import { getCanonicalLoopbackCloudUrl } from "#/lib/server-url";

const NEW_SESSION_TIMEOUT_MS = 15_000;

export const Route = createFileRoute("/sessions")({
	beforeLoad: async () => {
		const canonicalUrl = getCanonicalLoopbackCloudUrl();
		if (canonicalUrl) throw redirect({ href: canonicalUrl });
		const session = await getCachedCurrentSession();
		if (!session) throw redirect({ to: "/" });
		return { session };
	},
	component: SessionsShell,
});

interface PendingNewSession {
	requestId: string;
	projectKey: string;
	computerId: string;
	projectId: string;
	knownSessionIds: Set<string>;
}

function SessionsShell() {
	const { session } = Route.useRouteContext();
	const activeTarget = useActiveSessionTarget();
	const shell = useSessionsShellViewModel(
		session as CloudSession,
		activeTarget,
	);
	const projectsForTopBar = useMemo(
		() => projectsForTarget(shell.projects, activeTarget),
		[shell.projects, activeTarget],
	);
	const topBarTitle = shell.activeSession?.title ?? "Desktop sessions";
	const topBarProjectLabel = shell.activeSession?.projectName ?? undefined;
	const [topBarAccessoryElement, setTopBarAccessoryElement] =
		useState<HTMLDivElement | null>(null);
	const shellContext = useMemo<SessionsShellContextValue>(
		() => ({
			...shell,
			topBarAccessoryElement,
		}),
		[shell, topBarAccessoryElement],
	);

	return (
		<SessionsShellContext.Provider value={shellContext}>
			<div className="flex h-dvh overflow-hidden bg-background text-foreground">
				<SessionSidebar
					session={session as CloudSession}
					visibleSessions={shell.visibleSessions}
					projects={projectsForTopBar}
					currentSessionKey={shell.activeSessionKey}
					onSelectSession={shell.selectSession}
					onSelectProject={shell.startSession}
					onArchiveSession={shell.archiveSession}
					onSignOut={handleSignOut}
					pendingProjectKey={shell.pendingProjectKey}
					isSessionDirectoryLoading={shell.isSessionDirectoryLoading}
				/>
				<main className="flex h-dvh min-w-0 flex-1 flex-col overflow-hidden">
					<SessionTopBar
						title={topBarTitle}
						projectLabel={topBarProjectLabel}
						emphasizeTitle={shell.activeSession?.isComputerUse ?? false}
						liftContent={shell.activeSession?.isComputerUse ?? false}
						accessorySlotRef={setTopBarAccessoryElement}
						drawerTrigger={
							<SessionDrawer
								session={session as CloudSession}
								visibleSessions={shell.visibleSessions}
								projects={projectsForTopBar}
								currentSessionKey={shell.activeSessionKey}
								onSelectSession={shell.selectSession}
								onSelectProject={shell.startSession}
								onArchiveSession={shell.archiveSession}
								onSignOut={handleSignOut}
								pendingProjectKey={shell.pendingProjectKey}
								isSessionDirectoryLoading={shell.isSessionDirectoryLoading}
								trigger={
									<Button
										type="button"
										variant="ghost"
										size="icon"
										aria-label="Open sessions list"
										className="text-muted-foreground hover:text-foreground lg:hidden"
									>
										<Menu className="h-4 w-4" />
									</Button>
								}
							/>
						}
					/>
					<div className="flex min-h-0 flex-1 overflow-hidden">
						{activeTarget ? <Outlet /> : <SessionsIndexContent />}
					</div>
				</main>
			</div>
		</SessionsShellContext.Provider>
	);

	function handleSignOut() {
		void signOut().then(() => {
			if (typeof window !== "undefined") window.location.href = "/";
		});
	}
}

function useSessionsShellViewModel(
	session: CloudSession,
	activeTarget: ActiveSessionTarget | null,
): SessionsShellContextValue {
	const navigate = useNavigate();
	const remoteSessions = useAccountRemoteSessions(
		session.relayToken,
		session.accountId,
		session.devices,
		session.deviceId,
	);
	const visibleSessions = remoteSessions.sessions;
	const activeSessionKey = activeTarget
		? sessionKey(activeTarget.computerId, activeTarget.sessionId)
		: null;
	const activeSession =
		activeTarget === null
			? null
			: (visibleSessions.find(
					(summary) =>
						summary.computerId === activeTarget.computerId &&
						summary.sessionId === activeTarget.sessionId,
				) ?? null);
	const activeProjectLabel =
		activeSession?.projectName ?? activeSession?.projectId ?? "this project";
	const activeComputerId = activeTarget?.computerId ?? null;
	const computerPresenceKnown = useSessionStore(
		(state) => state.computerPresenceKnown,
	);
	const currentComputerOnline = useSessionStore((state) =>
		activeComputerId
			? Boolean(state.onlineComputerIds[activeComputerId])
			: false,
	);
	const currentComputerReconciled = useSessionStore((state) =>
		activeComputerId
			? Boolean(state.visibleSessionsByComputerVersion[activeComputerId])
			: false,
	);
	const currentComputerRemoteControl = activeComputerId
		? (remoteSessions.remoteControlByComputer[activeComputerId] ?? null)
		: null;
	const desktopDeviceIds = useMemo(
		() =>
			session.devices
				.filter((device) => device.kind === "desktop" && !device.revoked_at)
				.map((device) => device.id),
		[session.devices],
	);
	const onlineComputerIds = useSessionStore((state) => state.onlineComputerIds);
	const visibleSessionsByComputerVersion = useSessionStore(
		(state) => state.visibleSessionsByComputerVersion,
	);
	const isSessionDirectoryLoading = useMemo(() => {
		if (desktopDeviceIds.length === 0) return false;
		if (!computerPresenceKnown) return true;
		return desktopDeviceIds.some(
			(computerId) =>
				onlineComputerIds[computerId] &&
				!visibleSessionsByComputerVersion[computerId],
		);
	}, [
		computerPresenceKnown,
		desktopDeviceIds,
		onlineComputerIds,
		visibleSessionsByComputerVersion,
	]);
	const visibleSessionsVersion = useSessionStore(
		(state) => state.visibleSessionsVersion,
	);
	const activeTargetValid = activeTarget === null || Boolean(activeSession);
	const [pendingNewSession, setPendingNewSession] =
		useState<PendingNewSession | null>(null);
	const [invalidActiveTargetKey, setInvalidActiveTargetKey] = useState<
		string | null
	>(null);
	const redirectedSessionKey = useRef<string | null>(null);

	const selectSession = useCallback(
		(computerId: string, sessionId: string) => {
			void navigate({
				to: "/sessions/$computerId/$sessionId",
				params: { computerId, sessionId },
			});
		},
		[navigate],
	);

	const startSession = useCallback(
		(
			project: RemoteProjectSummary,
			options?: { sessionKind?: SessionKind },
		) => {
			if (pendingNewSession) return;
			const knownSessionIds = new Set(
				visibleSessions
					.filter(
						(summary) =>
							summary.computerId === project.computerId &&
							summary.projectId === project.projectId,
					)
					.map((summary) => summary.sessionId),
			);
			const didRequest = options
				? remoteSessions.startSession(project, options)
				: remoteSessions.startSession(project);
			if (!didRequest) return;
			setPendingNewSession({
				requestId: `${Date.now()}:${projectKey(project)}`,
				projectKey: projectKey(project),
				computerId: project.computerId,
				projectId: project.projectId,
				knownSessionIds,
			});
		},
		[pendingNewSession, remoteSessions, visibleSessions],
	);

	const reportActiveTargetInvalid = useCallback((targetKey: string) => {
		setInvalidActiveTargetKey(targetKey);
	}, []);

	useEffect(() => {
		if (activeTarget || pendingNewSession) return;
		const first = visibleSessions[0];
		if (!first) return;
		const nextSessionKey = sessionKey(first.computerId, first.sessionId);
		if (redirectedSessionKey.current === nextSessionKey) return;
		redirectedSessionKey.current = nextSessionKey;
		void navigate({
			to: "/sessions/$computerId/$sessionId",
			params: { computerId: first.computerId, sessionId: first.sessionId },
			replace: true,
		});
	}, [activeTarget, navigate, pendingNewSession, visibleSessions]);

	useEffect(() => {
		if (!pendingNewSession) return;
		const created = visibleSessions.find(
			(summary) =>
				summary.computerId === pendingNewSession.computerId &&
				summary.projectId === pendingNewSession.projectId &&
				!pendingNewSession.knownSessionIds.has(summary.sessionId),
		);
		if (!created) return;
		setPendingNewSession(null);
		void navigate({
			to: "/sessions/$computerId/$sessionId",
			params: {
				computerId: created.computerId,
				sessionId: created.sessionId,
			},
			replace: true,
		});
	}, [navigate, pendingNewSession, visibleSessions]);

	useEffect(() => {
		if (!pendingNewSession) return;
		const requestId = pendingNewSession.requestId;
		const timeout = window.setTimeout(() => {
			setPendingNewSession((current) =>
				current?.requestId === requestId ? null : current,
			);
		}, NEW_SESSION_TIMEOUT_MS);
		return () => window.clearTimeout(timeout);
	}, [pendingNewSession]);

	useEffect(() => {
		if (!activeTarget || !activeSessionKey) return;
		const invalidByPresence = computerPresenceKnown && !currentComputerOnline;
		const invalidByDirectory = currentComputerReconciled && !activeSession;
		const invalidByChildReport = invalidActiveTargetKey === activeSessionKey;
		if (!invalidByPresence && !invalidByDirectory && !invalidByChildReport) {
			return;
		}
		setInvalidActiveTargetKey((current) =>
			current === activeSessionKey ? null : current,
		);
		void navigate({ to: "/sessions", replace: true });
	}, [
		activeSession,
		activeSessionKey,
		activeTarget,
		computerPresenceKnown,
		currentComputerOnline,
		currentComputerReconciled,
		invalidActiveTargetKey,
		navigate,
	]);

	return useMemo(
		() => ({
			session,
			visibleSessions,
			projects: remoteSessions.projects,
			activeTarget,
			activeSession,
			activeSessionKey,
			activeProjectLabel,
			activeTargetValid,
			computerPresenceKnown,
			currentComputerOnline,
			currentComputerReconciled,
			currentComputerRemoteControl,
			isSessionDirectoryLoading,
			remoteControlByComputer: remoteSessions.remoteControlByComputer,
			topBarAccessoryElement: null,
			visibleSessionsVersion,
			selectSession,
			startSession,
			archiveSession: remoteSessions.archiveSession,
			clearComputerUseChat: remoteSessions.clearComputerUseChat,
			reportActiveTargetInvalid,
			pendingProjectKey: pendingNewSession?.projectKey ?? null,
		}),
		[
			activeProjectLabel,
			activeSession,
			activeSessionKey,
			activeTarget,
			activeTargetValid,
			computerPresenceKnown,
			currentComputerOnline,
			currentComputerReconciled,
			currentComputerRemoteControl,
			isSessionDirectoryLoading,
			pendingNewSession,
			remoteSessions,
			reportActiveTargetInvalid,
			selectSession,
			session,
			startSession,
			visibleSessions,
			visibleSessionsVersion,
		],
	);
}

function SessionsIndexContent() {
	const {
		visibleSessions,
		selectSession,
		archiveSession,
		isSessionDirectoryLoading,
	} = useSessionsShell();
	const [pendingSessionAction, setPendingSessionAction] = useState<{
		key: string;
		action: "archive";
	} | null>(null);
	const hasSessions = visibleSessions.length > 0;
	const showDirectoryLoading = isSessionDirectoryLoading && !hasSessions;

	const openSession = (summary: VisibleSessionSummary) => {
		selectSession(summary.computerId, summary.sessionId);
	};
	const handleArchiveSession = (summary: VisibleSessionSummary) => {
		const key = sessionKey(summary.computerId, summary.sessionId);
		setPendingSessionAction({ key, action: "archive" });
		try {
			archiveSession(summary);
		} finally {
			setPendingSessionAction(null);
		}
	};

	return (
		<div className="relative flex min-h-0 w-full flex-1 items-center justify-center px-6 py-12">
			<Empty className="border-0">
				<EmptyHeader>
					{showDirectoryLoading ? (
						<EmptyMedia
							variant="icon"
							className="cloud-halo-soft size-16 border-border/60 bg-card/40"
						>
							<Skeleton className="size-8 rounded-[6px] bg-primary/25" />
						</EmptyMedia>
					) : (
						<EmptyMedia className="cloud-halo size-16 border-0 bg-transparent">
							<BrandLogo className="size-10" aria-label="Xero" />
						</EmptyMedia>
					)}
					<EmptyTitle className="font-display mt-4 text-[26px] font-medium leading-tight tracking-[-0.02em] text-foreground">
						{showDirectoryLoading ? (
							<>Loading desktop sessions</>
						) : hasSessions ? (
							<>
								Open a{" "}
								<em className="font-display-italic text-primary">
									desktop session
								</em>
							</>
						) : (
							<>
								No desktop sessions{" "}
								<em className="font-display-italic text-primary">yet</em>
							</>
						)}
					</EmptyTitle>
					<EmptyDescription className="mx-auto mt-1 max-w-sm text-[13px] leading-relaxed text-muted-foreground">
						{showDirectoryLoading
							? "Your desktop sessions will appear here as soon as Xero finishes syncing."
							: hasSessions
								? "Conversation content stays on the desktop until you open a session."
								: "Open Xero on your desktop to make your coding sessions available here."}
					</EmptyDescription>
				</EmptyHeader>
				{showDirectoryLoading ? (
					<EmptyContent>
						<output
							aria-label="Loading desktop sessions"
							className="flex w-[min(28rem,calc(100vw-2rem))] flex-col gap-2 rounded-md border border-border/70 bg-card/35 p-2 backdrop-blur-sm lg:hidden"
						>
							<Skeleton className="h-10 rounded-md bg-accent/45" />
							<Skeleton className="h-10 rounded-md bg-accent/35" />
							<Skeleton className="h-10 rounded-md bg-accent/30" />
						</output>
					</EmptyContent>
				) : hasSessions ? (
					<EmptyContent>
						<ul className="flex w-[min(28rem,calc(100vw-2rem))] flex-col gap-1 overflow-hidden rounded-xl border border-border/70 bg-card/40 p-1 backdrop-blur-sm lg:hidden">
							{visibleSessions.map((summary) => {
								const key = sessionKey(summary.computerId, summary.sessionId);
								const pendingAction =
									pendingSessionAction?.key === key
										? pendingSessionAction.action
										: undefined;
								return (
									<li key={key}>
										<SessionListRow
											summary={summary}
											isActive={false}
											onSelect={() => openSession(summary)}
											onArchive={() => handleArchiveSession(summary)}
											isPending={pendingSessionAction?.key === key}
											pendingAction={pendingAction}
										/>
									</li>
								);
							})}
						</ul>
					</EmptyContent>
				) : null}
			</Empty>
		</div>
	);
}

function useActiveSessionTarget(): ActiveSessionTarget | null {
	return useRouterState({
		select: (state) => activeSessionTargetFromPathname(state.location.pathname),
	});
}

export function activeSessionTargetFromPathname(
	pathname: string,
): ActiveSessionTarget | null {
	const match = /^\/sessions\/([^/]+)\/([^/]+)\/?$/.exec(pathname);
	if (!match) return null;
	return {
		computerId: decodeURIComponent(match[1]),
		sessionId: decodeURIComponent(match[2]),
	};
}

function projectsForTarget(
	projects: RemoteProjectSummary[],
	target: ActiveSessionTarget | null,
): RemoteProjectSummary[] {
	if (!target) return projects;
	return projects.filter((project) => project.computerId === target.computerId);
}

function projectKey(project: RemoteProjectSummary): string {
	return `${project.computerId}:${project.projectId}`;
}
