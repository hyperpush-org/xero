import {
	createFileRoute,
	Outlet,
	redirect,
	useNavigate,
	useRouterState,
} from "@tanstack/react-router";
import { AppLogo } from "@xero/ui/components/app-logo";
import { Button } from "@xero/ui/components/ui/button";
import { Menu } from "lucide-react";
import { useEffect, useRef } from "react";

import { SessionDrawer } from "#/components/session-drawer";
import {
	type CloudSession,
	getCurrentSession,
	signOut,
} from "#/lib/auth/session";
import { sessionKey } from "#/lib/relay/session-store";
import { useAccountVisibleSessions } from "#/lib/relay/use-session-stream";

export const Route = createFileRoute("/sessions")({
	beforeLoad: async () => {
		const session = await getCurrentSession();
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
	return <SessionsEmptyState />;
}

function SessionsEmptyState() {
	const { session } = Route.useRouteContext();
	const navigate = useNavigate();
	const redirectedSessionKey = useRef<string | null>(null);
	const visibleSessions = useAccountVisibleSessions(
		session.relayToken,
		session.accountId,
		session.devices,
		session.deviceId,
	);

	useEffect(() => {
		if (visibleSessions.length === 0) return;
		const first = visibleSessions[0];
		const nextSessionKey = sessionKey(first.computerId, first.sessionId);
		if (redirectedSessionKey.current === nextSessionKey) return;
		redirectedSessionKey.current = nextSessionKey;
		void navigate({
			to: "/sessions/$computerId/$sessionId",
			params: { computerId: first.computerId, sessionId: first.sessionId },
			replace: true,
		});
	}, [navigate, visibleSessions]);

	const handleSignOut = () => {
		void signOut().then(() => {
			if (typeof window !== "undefined") window.location.href = "/";
		});
	};

	return (
		<main className="flex min-h-dvh flex-col bg-background text-foreground">
			<header className="sticky top-0 z-20 flex items-center justify-between gap-2 border-b border-border/70 bg-background px-5 py-3">
				<div className="flex items-center gap-2">
					<AppLogo className="size-5" aria-label="Xero" />
					<span className="text-base font-semibold tracking-tight text-foreground">
						Xero
					</span>
				</div>
				<SessionDrawer
					session={session as CloudSession}
					visibleSessions={visibleSessions}
					currentSessionKey={null}
					onSelectSession={(computerId, sessionId) => {
						void navigate({
							to: "/sessions/$computerId/$sessionId",
							params: { computerId, sessionId },
						});
					}}
					onSignOut={handleSignOut}
					trigger={
						<Button
							type="button"
							variant="ghost"
							size="icon"
							aria-label="Open sessions list"
							className="text-muted-foreground hover:text-foreground"
						>
							<Menu className="h-5 w-5" />
						</Button>
					}
				/>
			</header>

			<div className="flex flex-1 flex-col items-center justify-center gap-8 px-6 py-12 text-center">
				<div className="flex size-16 items-center justify-center rounded-2xl border border-border/70 bg-secondary/30 animate-in fade-in-0 zoom-in-95 motion-enter">
					<AppLogo className="size-8" aria-label="Xero" />
				</div>

				<div className="flex max-w-sm flex-col items-center gap-2.5 animate-in fade-in-0 slide-in-from-bottom-2 motion-enter [animation-delay:80ms] [animation-fill-mode:both]">
					<h1 className="text-balance text-[22px] font-semibold leading-tight tracking-tight text-foreground sm:text-2xl">
						No sessions are shared yet
					</h1>
					<p className="text-pretty text-sm leading-relaxed text-muted-foreground">
						Open the Xero desktop app and toggle{" "}
						<span className="font-medium text-foreground">Share to web</span> on
						a session row to drive it from here.
					</p>
				</div>

				<div className="animate-in fade-in-0 motion-enter [animation-delay:140ms] [animation-fill-mode:both]">
					<SessionDrawer
						session={session as CloudSession}
						visibleSessions={visibleSessions}
						currentSessionKey={null}
						onSelectSession={(computerId, sessionId) => {
							void navigate({
								to: "/sessions/$computerId/$sessionId",
								params: { computerId, sessionId },
							});
						}}
						onSignOut={handleSignOut}
						trigger={
							<Button
								type="button"
								size="sm"
								variant="secondary"
								className="h-9 border border-border/70 bg-secondary/60 px-4 text-secondary-foreground hover:bg-secondary"
							>
								<Menu className="h-3.5 w-3.5" />
								Open menu
							</Button>
						}
					/>
				</div>
			</div>
		</main>
	);
}
