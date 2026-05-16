import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { Button } from "@xero/ui/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@xero/ui/components/ui/tooltip";
import { Cloud, Menu } from "lucide-react";
import { useEffect } from "react";

import { SessionDrawer } from "#/components/session-drawer";
import {
	type CloudSession,
	getCurrentSession,
	signOut,
} from "#/lib/auth/session";
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
	const { session } = Route.useRouteContext();
	const navigate = useNavigate();
	const visibleSessions = useAccountVisibleSessions(
		session.relayToken,
		session.accountId,
	);

	// When the first visible session becomes known, auto-navigate to it so the
	// user lands on a conversation immediately.
	useEffect(() => {
		if (visibleSessions.length === 0) return;
		const first = visibleSessions[0];
		void navigate({
			to: "/sessions/$computerId/$sessionId",
			params: { computerId: first.computerId, sessionId: first.sessionId },
			replace: true,
		});
	}, [navigate, visibleSessions]);

	return (
		<main className="flex min-h-dvh flex-col bg-background text-foreground">
			<header className="sticky top-0 z-20 flex items-center justify-between gap-2 border-b border-border bg-background/95 px-4 py-3">
				<h1 className="text-base font-medium">Xero</h1>
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
					onSignOut={() => {
						void signOut().then(() => {
							if (typeof window !== "undefined") window.location.href = "/";
						});
					}}
					trigger={
						<Button variant="ghost" size="icon" aria-label="Open sessions list">
							<Menu className="h-5 w-5" />
						</Button>
					}
				/>
			</header>
			<div className="flex flex-1 flex-col items-center justify-center gap-4 px-6 py-10 text-center text-sm text-muted-foreground">
				<Tooltip>
					<TooltipTrigger asChild>
						<Cloud className="h-12 w-12 text-muted-foreground/50" aria-hidden />
					</TooltipTrigger>
					<TooltipContent>Waiting for a shared session</TooltipContent>
				</Tooltip>
				<p className="max-w-sm">
					No sessions are shared to the web yet. Open the Xero desktop app and
					toggle{" "}
					<span className="font-medium text-foreground">Share to web</span> on a
					session row.
				</p>
			</div>
		</main>
	);
}
