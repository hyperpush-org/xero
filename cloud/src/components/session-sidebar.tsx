import { Link } from "@tanstack/react-router";
import { ResizableSidebar } from "@xero/ui/components/resizable-sidebar";
import { cn } from "@xero/ui/lib/utils";

import { BrandLogo } from "#/components/brand-logo";
import type { CloudSession } from "#/lib/auth/session";
import type {
	RemoteProjectSummary,
	SessionKind,
	VisibleSessionSummary,
} from "#/lib/relay/session-store";

import { SessionListPanel } from "./session-list-panel";

const SESSIONS_SIDEBAR_STORAGE_KEY = "xero.cloud.sessionsSidebar.width.v1";
const SESSIONS_SIDEBAR_DEFAULT_WIDTH = 276;
const SESSIONS_SIDEBAR_MIN_WIDTH = 232;
const SESSIONS_SIDEBAR_MAX_WIDTH = 520;

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
	isSessionDirectoryLoading?: boolean;
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
	isSessionDirectoryLoading = false,
	className,
}: SessionSidebarProps) {
	return (
		<ResizableSidebar
			aria-label="Desktop sessions"
			defaultWidth={SESSIONS_SIDEBAR_DEFAULT_WIDTH}
			maxWidth={SESSIONS_SIDEBAR_MAX_WIDTH}
			minWidth={SESSIONS_SIDEBAR_MIN_WIDTH}
			resizable
			resizeEdge="right"
			resizeLabel="Resize sessions sidebar"
			widthStorageKey={SESSIONS_SIDEBAR_STORAGE_KEY}
			className={cn(
				"hidden gap-0 border-r border-border/70 lg:flex",
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
				isSessionDirectoryLoading={isSessionDirectoryLoading}
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
		</ResizableSidebar>
	);
}
