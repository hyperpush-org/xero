import { Link } from "@tanstack/react-router";
import { cn } from "@xero/ui/lib/utils";

import { BrandLogo } from "#/components/brand-logo";
import type { CloudSession } from "#/lib/auth/session";
import type {
	RemoteProjectSummary,
	SessionKind,
	VisibleSessionSummary,
} from "#/lib/relay/session-store";

import { SessionListPanel } from "./session-list-panel";

interface SessionSidebarProps {
	session: CloudSession;
	visibleSessions: VisibleSessionSummary[];
	projects?: RemoteProjectSummary[];
	currentSessionKey: string | null;
	onSelectSession: (computerId: string, sessionId: string) => void;
	onSelectProject?: (
		project: RemoteProjectSummary,
		options?: { sessionKind?: SessionKind },
	) => void;
	onArchiveSession?: (
		summary: VisibleSessionSummary,
	) => boolean | Promise<boolean>;
	onSignOut: () => void;
	pendingProjectKey?: string | null;
	className?: string;
}

export function SessionSidebar({
	session,
	visibleSessions,
	projects = [],
	currentSessionKey,
	onSelectSession,
	onSelectProject,
	onArchiveSession,
	onSignOut,
	pendingProjectKey,
	className,
}: SessionSidebarProps) {
	return (
		<aside
			aria-label="Desktop sessions"
			className={cn(
				"relative hidden h-dvh w-[276px] shrink-0 flex-col gap-0 border-r border-border/70 bg-sidebar lg:flex",
				className,
			)}
		>
			<SessionListPanel
				session={session}
				visibleSessions={visibleSessions}
				projects={projects}
				currentSessionKey={currentSessionKey}
				onSelectSession={onSelectSession}
				onSelectProject={onSelectProject}
				onArchiveSession={onArchiveSession}
				onSignOut={onSignOut}
				pendingProjectKey={pendingProjectKey}
				showCount={false}
				inlineProjectActions
				titleSlot={
					<Link
						to="/sessions"
						className="group relative flex min-w-0 items-center gap-2.5 rounded-md px-1 py-0.5 -mx-1 transition-colors hover:bg-accent/40"
						aria-label="Xero"
					>
						<BrandLogo className="size-4 shrink-0" aria-hidden />
						<span className="font-display truncate text-[14px] font-medium tracking-tight text-foreground">
							Xero
						</span>
					</Link>
				}
			/>
		</aside>
	);
}
