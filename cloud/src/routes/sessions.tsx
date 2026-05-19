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
import { Menu } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { BrandLogo } from "#/components/brand-logo";
import { SessionDrawer } from "#/components/session-drawer";
import { SessionListRow } from "#/components/session-list-row";
import {
	type CloudSession,
	getCachedCurrentSession,
	signOut,
} from "#/lib/auth/session";
import {
	sessionKey,
	type VisibleSessionSummary,
} from "#/lib/relay/session-store";
import { useAccountRemoteSessions } from "#/lib/relay/use-session-stream";
import { getCanonicalLoopbackCloudUrl } from "#/lib/server-url";

export const Route = createFileRoute("/sessions")({
	beforeLoad: async () => {
		const canonicalUrl = getCanonicalLoopbackCloudUrl();
		if (canonicalUrl) throw redirect({ href: canonicalUrl });
		const session = await getCachedCurrentSession();
		if (!session) throw redirect({ to: "/" });
		return { session };
	},
	component: SessionsIndex,
});

function SessionsIndex() {
	const isSessionsIndex = useRouterState({
		select: (state) => {
			const pathname = state.location.pathname;
			return pathname === "/sessions" || pathname === "/sessions/";
		},
	});
	if (!isSessionsIndex) return <Outlet />;
	return <SessionsHome />;
}

function SessionsHome() {
	const { session } = Route.useRouteContext();
	const remoteSessions = useAccountRemoteSessions(
		session.relayToken,
		session.accountId,
		session.devices,
		session.deviceId,
	);
	return (
		<SessionsEmptyState
			session={session as CloudSession}
			visibleSessions={remoteSessions.sessions}
			onSetSessionRemoteVisibility={remoteSessions.setSessionRemoteVisibility}
			onArchiveSession={remoteSessions.archiveSession}
		/>
	);
}

function SessionsEmptyState({
	session,
	visibleSessions,
	onSetSessionRemoteVisibility,
	onArchiveSession,
}: {
	session: CloudSession;
	visibleSessions: VisibleSessionSummary[];
	onSetSessionRemoteVisibility: (
		summary: VisibleSessionSummary,
		visible: boolean,
	) => boolean;
	onArchiveSession: (summary: VisibleSessionSummary) => boolean;
}) {
	const navigate = useNavigate();
	const redirectedSessionKey = useRef<string | null>(null);
	const [pendingSessionAction, setPendingSessionAction] = useState<{
		key: string;
		action: "visibility" | "archive";
	} | null>(null);
	const linkedSessions = visibleSessions.filter(
		(summary) => summary.remoteVisible,
	);

	useEffect(() => {
		if (linkedSessions.length === 0) return;
		const first = linkedSessions[0];
		const nextSessionKey = sessionKey(first.computerId, first.sessionId);
		if (redirectedSessionKey.current === nextSessionKey) return;
		redirectedSessionKey.current = nextSessionKey;
		void navigate({
			to: "/sessions/$computerId/$sessionId",
			params: { computerId: first.computerId, sessionId: first.sessionId },
			replace: true,
		});
	}, [linkedSessions, navigate]);

	const handleSignOut = () => {
		void signOut().then(() => {
			if (typeof window !== "undefined") window.location.href = "/";
		});
	};
	const navigateToSession = (computerId: string, sessionId: string) => {
		void navigate({
			to: "/sessions/$computerId/$sessionId",
			params: { computerId, sessionId },
		});
	};
	const openSession = (summary: VisibleSessionSummary) => {
		if (!summary.remoteVisible) {
			const key = sessionKey(summary.computerId, summary.sessionId);
			setPendingSessionAction({ key, action: "visibility" });
			try {
				const didRequest = onSetSessionRemoteVisibility(summary, true);
				if (!didRequest) return;
			} finally {
				setPendingSessionAction(null);
			}
		}
		navigateToSession(summary.computerId, summary.sessionId);
	};
	const handleSetSessionRemoteVisibility = async (
		summary: VisibleSessionSummary,
		visible: boolean,
	) => {
		const key = sessionKey(summary.computerId, summary.sessionId);
		setPendingSessionAction({ key, action: "visibility" });
		try {
			onSetSessionRemoteVisibility(summary, visible);
		} finally {
			setPendingSessionAction(null);
		}
	};
	const handleArchiveSession = async (summary: VisibleSessionSummary) => {
		const key = sessionKey(summary.computerId, summary.sessionId);
		setPendingSessionAction({ key, action: "archive" });
		try {
			onArchiveSession(summary);
		} finally {
			setPendingSessionAction(null);
		}
	};

	return (
		<main className="flex min-h-dvh flex-col bg-background text-foreground">
			<header className="sticky top-0 z-20 flex items-center justify-between gap-2 bg-background px-4 py-3">
				<div className="flex items-center gap-2">
					<BrandLogo className="size-5" aria-label="Xero" />
					<span className="text-sm font-medium tracking-tight text-foreground">
						Xero
					</span>
				</div>
				<SessionDrawer
					session={session as CloudSession}
					visibleSessions={visibleSessions}
					currentSessionKey={null}
					onSelectSession={navigateToSession}
					onSetSessionRemoteVisibility={onSetSessionRemoteVisibility}
					onArchiveSession={onArchiveSession}
					onSignOut={handleSignOut}
					trigger={
						<Button
							type="button"
							variant="ghost"
							size="icon"
							aria-label="Open sessions list"
							className="text-muted-foreground hover:text-foreground"
						>
							<Menu className="h-4 w-4" />
						</Button>
					}
				/>
			</header>

			<div className="flex min-h-full w-full flex-1 items-center justify-center">
				<Empty className="border-0">
					<EmptyHeader>
						<EmptyMedia>
							<BrandLogo className="size-10" aria-label="Xero" />
						</EmptyMedia>
						<EmptyTitle className="text-sm font-medium text-foreground">
							{visibleSessions.length > 0
								? "Open a desktop session"
								: "No desktop sessions yet"}
						</EmptyTitle>
						<EmptyDescription className="text-xs">
							{visibleSessions.length > 0
								? "Conversation content stays on the desktop until you open a session."
								: "Open Xero on your desktop to make sessions available here."}
						</EmptyDescription>
					</EmptyHeader>
					<EmptyContent>
						{visibleSessions.length > 0 ? (
							<ul className="flex w-[min(28rem,calc(100vw-2rem))] flex-col gap-1 rounded-lg border border-border bg-background p-1">
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
												onSetRemoteVisibility={(visible) =>
													void handleSetSessionRemoteVisibility(
														summary,
														visible,
													)
												}
												onArchive={() => void handleArchiveSession(summary)}
												isPending={pendingSessionAction?.key === key}
												pendingAction={pendingAction}
											/>
										</li>
									);
								})}
							</ul>
						) : (
							<SessionDrawer
								session={session as CloudSession}
								visibleSessions={visibleSessions}
								currentSessionKey={null}
								onSelectSession={navigateToSession}
								onSetSessionRemoteVisibility={onSetSessionRemoteVisibility}
								onArchiveSession={onArchiveSession}
								onSignOut={handleSignOut}
								trigger={
									<Button
										type="button"
										size="sm"
										variant="secondary"
										className="h-9 gap-2 px-4 text-[12px] font-medium"
									>
										<Menu className="h-3.5 w-3.5" />
										Open menu
									</Button>
								}
							/>
						)}
					</EmptyContent>
				</Empty>
			</div>
		</main>
	);
}
