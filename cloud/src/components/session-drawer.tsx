import {
	Avatar,
	AvatarFallback,
	AvatarImage,
} from "@xero/ui/components/ui/avatar";
import { Button } from "@xero/ui/components/ui/button";
import {
	Sheet,
	SheetContent,
	SheetHeader,
	SheetTitle,
	SheetTrigger,
} from "@xero/ui/components/ui/sheet";
import { Menu, Power } from "lucide-react";
import type { ReactNode } from "react";

import type { CloudSession } from "#/lib/auth/session";
import type { VisibleSessionSummary } from "#/lib/relay/session-store";

import { SessionListRow } from "./session-list-row";

interface SessionDrawerProps {
	session: CloudSession;
	visibleSessions: VisibleSessionSummary[];
	currentSessionKey: string | null;
	onSelectSession: (computerId: string, sessionId: string) => void;
	onSignOut: () => void;
	trigger?: ReactNode;
}

export function SessionDrawer({
	session,
	visibleSessions,
	currentSessionKey,
	onSelectSession,
	onSignOut,
	trigger,
}: SessionDrawerProps) {
	return (
		<Sheet>
			<SheetTrigger asChild>
				{trigger ?? (
					<Button variant="ghost" size="icon" aria-label="Open sessions list">
						<Menu className="h-5 w-5" />
					</Button>
				)}
			</SheetTrigger>
			<SheetContent
				side="right"
				className="flex w-[88vw] max-w-sm flex-col gap-0 p-0"
			>
				<SheetHeader className="border-b border-border px-5 py-4">
					<SheetTitle className="text-base font-medium">
						List of sessions
					</SheetTitle>
				</SheetHeader>
				<div className="flex-1 overflow-y-auto px-2 py-2">
					{visibleSessions.length === 0 ? (
						<p className="px-3 py-6 text-center text-sm text-muted-foreground">
							Toggle &quot;Share to web&quot; on a session in the desktop app to
							see it here.
						</p>
					) : (
						<ul className="flex flex-col gap-1">
							{visibleSessions.map((summary) => {
								const key = `${summary.computerId}:${summary.sessionId}`;
								return (
									<li key={key}>
										<SessionListRow
											summary={summary}
											isActive={currentSessionKey === key}
											onSelect={() =>
												onSelectSession(summary.computerId, summary.sessionId)
											}
										/>
									</li>
								);
							})}
						</ul>
					)}
				</div>
				<footer className="flex items-center justify-between border-t border-border px-4 py-3 pb-[max(env(safe-area-inset-bottom),0.75rem)]">
					<div className="flex items-center gap-3 min-w-0">
						<Avatar className="h-8 w-8">
							{session.avatarUrl ? (
								<AvatarImage
									src={session.avatarUrl}
									alt={session.githubLogin}
								/>
							) : null}
							<AvatarFallback className="text-xs">
								{session.githubLogin.slice(0, 2).toUpperCase()}
							</AvatarFallback>
						</Avatar>
						<span className="truncate text-sm font-medium">
							{session.githubLogin}
						</span>
					</div>
					<Button
						variant="ghost"
						size="icon"
						aria-label="Sign out"
						onClick={onSignOut}
						className="text-muted-foreground hover:text-destructive"
					>
						<Power className="h-4 w-4" />
					</Button>
				</footer>
			</SheetContent>
		</Sheet>
	);
}
