import { Button } from "@xero/ui/components/ui/button";
import {
	Sheet,
	SheetClose,
	SheetContent,
	SheetDescription,
	SheetHeader,
	SheetTitle,
	SheetTrigger,
} from "@xero/ui/components/ui/sheet";
import { Menu, X } from "lucide-react";
import { type ReactNode, useCallback, useState } from "react";

import type { CloudSession } from "#/lib/auth/session";
import type {
	RemoteProjectSummary,
	SessionKind,
	VisibleSessionSummary,
} from "#/lib/relay/session-store";

import { SessionListPanel } from "./session-list-panel";

interface SessionDrawerProps {
	session: CloudSession;
	visibleSessions: VisibleSessionSummary[];
	projects?: RemoteProjectSummary[];
	currentSessionKey: string | null;
	open?: boolean;
	onOpenChange?: (open: boolean) => void;
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
	trigger?: ReactNode;
}

export function SessionDrawer({
	session,
	visibleSessions,
	projects = [],
	currentSessionKey,
	open,
	onOpenChange,
	onSelectSession,
	onSelectProject,
	onArchiveSession,
	onSignOut,
	pendingProjectKey,
	trigger,
}: SessionDrawerProps) {
	const [internalOpen, setInternalOpen] = useState(false);
	const isOpen = open ?? internalOpen;
	const setIsOpen = useCallback(
		(next: boolean) => {
			setInternalOpen(next);
			onOpenChange?.(next);
		},
		[onOpenChange],
	);
	const handleSelectProject = useCallback(
		(
			project: RemoteProjectSummary,
			options?: { sessionKind?: SessionKind },
		) => {
			if (options) {
				onSelectProject?.(project, options);
			} else {
				onSelectProject?.(project);
			}
			setIsOpen(false);
		},
		[onSelectProject, setIsOpen],
	);

	return (
		<Sheet open={isOpen} onOpenChange={setIsOpen}>
			<SheetTrigger asChild>
				{trigger ?? (
					<Button variant="ghost" size="icon" aria-label="Open sessions list">
						<Menu className="h-5 w-5" />
					</Button>
				)}
			</SheetTrigger>
			<SheetContent
				side="right"
				onOpenAutoFocus={(event) => event.preventDefault()}
				className="cloud-session-drawer-content flex w-[86vw] max-w-[340px] flex-col gap-0 border-l border-border/80 bg-sidebar p-0 sm:w-[340px] [&>button.absolute]:hidden"
			>
				<SheetHeader className="sr-only">
					<SheetTitle>Desktop sessions</SheetTitle>
					<SheetDescription>
						Browse desktop sessions and manage the signed-in account.
					</SheetDescription>
				</SheetHeader>
				<SessionListPanel
					session={session}
					visibleSessions={visibleSessions}
					projects={projects}
					currentSessionKey={currentSessionKey}
					onSelectSession={onSelectSession}
					onSelectProject={onSelectProject ? handleSelectProject : undefined}
					onArchiveSession={onArchiveSession}
					onSignOut={onSignOut}
					pendingProjectKey={pendingProjectKey}
					alwaysShowRowActions
					onAfterSelectSession={() => setIsOpen(false)}
					closeSlot={
						<SheetClose asChild>
							<button
								type="button"
								aria-label="Close"
								className="-mr-1 flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
							>
								<X className="h-4 w-4" />
							</button>
						</SheetClose>
					}
				/>
			</SheetContent>
		</Sheet>
	);
}
