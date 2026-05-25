import {
	Avatar,
	AvatarFallback,
	AvatarImage,
} from "@xero/ui/components/ui/avatar";
import { Button } from "@xero/ui/components/ui/button";
import {
	Collapsible,
	CollapsibleContent,
	CollapsibleTrigger,
} from "@xero/ui/components/ui/collapsible";
import {
	Empty,
	EmptyDescription,
	EmptyHeader,
	EmptyMedia,
	EmptyTitle,
} from "@xero/ui/components/ui/empty";
import { cn } from "@xero/ui/lib/utils";
import {
	ArrowUpRight,
	ChevronRight,
	Loader2,
	MessageSquare,
	Plus,
	Power,
	X,
} from "lucide-react";
import {
	type ReactNode,
	useCallback,
	useEffect,
	useMemo,
	useState,
} from "react";

import { InstallAppAction } from "#/components/install-app-action";
import { NewSessionPicker } from "#/components/new-session-picker";
import type { CloudSession } from "#/lib/auth/session";
import {
	type RemoteProjectSummary,
	sessionKey,
	type VisibleSessionSummary,
} from "#/lib/relay/session-store";

import { SessionListRow } from "./session-list-row";

interface SessionListPanelProps {
	session: CloudSession;
	visibleSessions: VisibleSessionSummary[];
	projects?: RemoteProjectSummary[];
	currentSessionKey: string | null;
	onSelectSession: (computerId: string, sessionId: string) => void;
	onSelectProject?: (project: RemoteProjectSummary) => void;
	onArchiveSession?: (
		summary: VisibleSessionSummary,
	) => boolean | Promise<boolean>;
	onSignOut: () => void;
	pendingProjectKey?: string | null;
	onAfterSelectSession?: () => void;
	onProjectPickerOpenChange?: (open: boolean) => void;
	titleAs?: "h2" | "div";
	titleSlot?: ReactNode;
	closeSlot?: ReactNode;
	showCount?: boolean;
	headerClassName?: string;
	alwaysShowRowActions?: boolean;
	/** Desktop sidebar: drop the header picker and add a per-project new-session button. */
	inlineProjectActions?: boolean;
}

const COLLAPSED_GROUPS_STORAGE_KEY = "xero.cloud.sidebar.projectCollapsed.v1";

interface ProjectGroup {
	key: string;
	computerId: string;
	projectId: string;
	label: string;
	sessions: VisibleSessionSummary[];
	lastActivityAt: number;
	containsActive: boolean;
}

function safeTimestamp(value: string | null): number {
	if (!value) return 0;
	const timestamp = Date.parse(value);
	return Number.isFinite(timestamp) ? timestamp : 0;
}

function groupSessionsByProject(
	sessions: readonly VisibleSessionSummary[],
	currentSessionKey: string | null,
): ProjectGroup[] {
	const groupsByKey = new Map<string, ProjectGroup>();
	for (const summary of sessions) {
		const key = `${summary.computerId}:${summary.projectId}`;
		const summaryKey = sessionKey(summary.computerId, summary.sessionId);
		const existing = groupsByKey.get(key);
		const activity = safeTimestamp(summary.lastActivityAt);
		const isActiveMember = currentSessionKey === summaryKey;
		if (existing) {
			existing.sessions.push(summary);
			if (activity > existing.lastActivityAt) {
				existing.lastActivityAt = activity;
			}
			if (isActiveMember) existing.containsActive = true;
		} else {
			groupsByKey.set(key, {
				key,
				computerId: summary.computerId,
				projectId: summary.projectId,
				label: summary.projectName ?? summary.projectId,
				sessions: [summary],
				lastActivityAt: activity,
				containsActive: isActiveMember,
			});
		}
	}
	return [...groupsByKey.values()].sort((left, right) => {
		return (
			right.lastActivityAt - left.lastActivityAt ||
			left.label.localeCompare(right.label) ||
			left.key.localeCompare(right.key)
		);
	});
}

function loadCollapsedGroups(): Record<string, boolean> {
	if (typeof window === "undefined") return {};
	try {
		const raw = window.localStorage.getItem(COLLAPSED_GROUPS_STORAGE_KEY);
		if (!raw) return {};
		const parsed = JSON.parse(raw) as unknown;
		if (!parsed || typeof parsed !== "object") return {};
		const result: Record<string, boolean> = {};
		for (const [key, value] of Object.entries(
			parsed as Record<string, unknown>,
		)) {
			if (typeof value === "boolean") result[key] = value;
		}
		return result;
	} catch {
		return {};
	}
}

export function SessionListPanel({
	session,
	visibleSessions,
	projects = [],
	currentSessionKey,
	onSelectSession,
	onSelectProject,
	onArchiveSession,
	onSignOut,
	pendingProjectKey = null,
	onAfterSelectSession,
	onProjectPickerOpenChange,
	titleAs = "div",
	titleSlot,
	closeSlot,
	showCount = true,
	headerClassName,
	alwaysShowRowActions = false,
	inlineProjectActions = false,
}: SessionListPanelProps) {
	const [pendingSessionAction, setPendingSessionAction] = useState<{
		key: string;
		action: "archive";
	} | null>(null);
	const [collapsedGroups, setCollapsedGroups] = useState<
		Record<string, boolean>
	>({});
	const [collapsedGroupsLoaded, setCollapsedGroupsLoaded] = useState(false);
	useEffect(() => {
		setCollapsedGroups(loadCollapsedGroups());
		setCollapsedGroupsLoaded(true);
	}, []);
	useEffect(() => {
		if (!collapsedGroupsLoaded || typeof window === "undefined") return;
		try {
			window.localStorage.setItem(
				COLLAPSED_GROUPS_STORAGE_KEY,
				JSON.stringify(collapsedGroups),
			);
		} catch {
			// Ignore quota / privacy-mode failures; collapse state is non-critical.
		}
	}, [collapsedGroups, collapsedGroupsLoaded]);
	const projectGroups = useMemo(
		() =>
			groupSessionsByProject(
				visibleSessions.filter((session) => !session.isComputerUse),
				currentSessionKey,
			),
		[visibleSessions, currentSessionKey],
	);
	const computerUseSessions = useMemo(
		() =>
			visibleSessions
				.filter((session) => session.isComputerUse)
				.sort(
					(left, right) =>
						(left.computerName ?? left.computerId).localeCompare(
							right.computerName ?? right.computerId,
						) || left.computerId.localeCompare(right.computerId),
				),
		[visibleSessions],
	);
	const total = visibleSessions.length;

	const handleSelectSession = useCallback(
		(summary: VisibleSessionSummary) => {
			onSelectSession(summary.computerId, summary.sessionId);
			onAfterSelectSession?.();
		},
		[onAfterSelectSession, onSelectSession],
	);
	const handleArchiveSession = useCallback(
		async (summary: VisibleSessionSummary) => {
			if (!onArchiveSession) return;
			const key = sessionKey(summary.computerId, summary.sessionId);
			setPendingSessionAction({ key, action: "archive" });
			try {
				await onArchiveSession(summary);
			} catch {
				// The desktop remains authoritative if the command fails.
			} finally {
				setPendingSessionAction(null);
			}
		},
		[onArchiveSession],
	);

	const TitleTag = titleAs;
	const titleNode = titleSlot ?? (
		<TitleTag className="font-display truncate text-[15px] font-medium tracking-tight text-foreground">
			Desktop sessions
		</TitleTag>
	);

	return (
		<>
			<div
				className={cn(
					"relative gap-0 border-b border-border/50 px-4 pb-2.5 pt-[max(env(safe-area-inset-top),0.6rem)] lg:py-3",
					headerClassName,
				)}
			>
				<div className="flex items-center justify-between gap-2">
					<div className="flex min-w-0 items-center gap-2.5">
						{titleNode}
						{showCount && total > 0 ? (
							<span className="text-cloud-meta tabular-nums text-muted-foreground/80">
								{total}
							</span>
						) : null}
					</div>
					<div className="flex shrink-0 items-center gap-1">
						{onSelectProject && !inlineProjectActions ? (
							<NewSessionPicker
								projects={projects}
								onSelectProject={onSelectProject}
								onPickerOpenChange={onProjectPickerOpenChange}
								pendingProjectKey={pendingProjectKey}
							/>
						) : null}
						{closeSlot}
					</div>
				</div>
			</div>

			<div className="flex flex-1 flex-col overflow-y-auto overscroll-contain">
				{total === 0 ? (
					<div className="flex min-h-full w-full flex-1 items-center justify-center px-6 py-8">
						<Empty className="border-0">
							<EmptyHeader>
								<EmptyMedia
									variant="icon"
									className="cloud-halo-soft size-12 border-border/60 bg-card/40"
								>
									<MessageSquare className="size-5 text-primary/80" />
								</EmptyMedia>
								<EmptyTitle className="font-display mt-3 text-[16px] font-medium tracking-tight text-foreground">
									No sessions yet
								</EmptyTitle>
								<EmptyDescription className="text-[12px] leading-relaxed">
									Open Xero on your desktop to make sessions
									<br className="hidden sm:inline" /> available here.
								</EmptyDescription>
							</EmptyHeader>
						</Empty>
					</div>
				) : (
					<div className="flex flex-col pb-2">
						{computerUseSessions.length > 0 ? (
							<ul className="flex flex-col pt-2">
								{computerUseSessions.map((summary) => {
									const key = sessionKey(summary.computerId, summary.sessionId);
									return (
										<li key={key}>
											<SessionListRow
												summary={summary}
												isActive={currentSessionKey === key}
												onSelect={() => handleSelectSession(summary)}
												isPending={false}
												hideProjectLabel
												compact
												alwaysShowActions={alwaysShowRowActions}
											/>
										</li>
									);
								})}
							</ul>
						) : null}
						{projectGroups.map((group, groupIndex) => {
							const userCollapsed = collapsedGroups[group.key] === true;
							const isOpen = group.containsActive || !userCollapsed;
							const count = group.sessions.length;
							return (
								<Collapsible
									key={group.key}
									open={isOpen}
									onOpenChange={(next) => {
										if (group.containsActive) return;
										setCollapsedGroups((current) => {
											const collapsed = !next;
											if ((current[group.key] === true) === collapsed) {
												return current;
											}
											const updated = { ...current };
											if (collapsed) {
												updated[group.key] = true;
											} else {
												delete updated[group.key];
											}
											return updated;
										});
									}}
									className={cn(
										"flex flex-col",
										groupIndex === 0 ? "pt-2" : "pt-3",
									)}
								>
									<div className="group/group flex w-full items-center gap-1 px-4 pb-1.5 pt-0.5">
										<CollapsibleTrigger
											aria-label={
												isOpen
													? `Collapse ${group.label}`
													: `Expand ${group.label}`
											}
											disabled={group.containsActive}
											className={cn(
												"flex min-w-0 flex-1 items-center gap-2 text-left transition-colors focus-visible:rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60",
												group.containsActive
													? "cursor-default"
													: "cursor-pointer",
											)}
										>
											<span className="min-w-0 flex-1 truncate text-[13.5px] font-medium tracking-tight text-muted-foreground/70 transition-colors group-hover/group:text-muted-foreground/90">
												{group.label}
											</span>
											{!isOpen ? (
												<span className="text-[10px] font-medium tabular-nums text-muted-foreground/45">
													{count}
												</span>
											) : null}
											<ChevronRight
												aria-hidden
												className={cn(
													"size-4 shrink-0 text-muted-foreground/45 transition-all duration-150 group-hover/group:text-muted-foreground/70",
													isOpen ? "rotate-90" : "rotate-0",
												)}
											/>
										</CollapsibleTrigger>
										{inlineProjectActions && onSelectProject ? (
											<button
												type="button"
												aria-label={`New session in ${group.label}`}
												title={`New session in ${group.label}`}
												onClick={() =>
													onSelectProject({
														computerId: group.computerId,
														projectId: group.projectId,
														projectName: group.label,
													})
												}
												disabled={pendingProjectKey !== null}
												className="inline-flex size-5 shrink-0 items-center justify-center rounded-md text-muted-foreground/45 transition-colors hover:bg-accent/50 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
											>
												{pendingProjectKey === group.key ? (
													<Loader2 className="size-4 animate-spin" />
												) : (
													<Plus className="size-4" />
												)}
											</button>
										) : null}
									</div>
									<CollapsibleContent className="overflow-hidden">
										<ul className="flex flex-col">
											{group.sessions.map((summary) => {
												const key = sessionKey(
													summary.computerId,
													summary.sessionId,
												);
												const pendingAction =
													pendingSessionAction?.key === key
														? pendingSessionAction.action
														: undefined;
												return (
													<li key={key}>
														<SessionListRow
															summary={summary}
															isActive={currentSessionKey === key}
															onSelect={() => handleSelectSession(summary)}
															onArchive={
																onArchiveSession
																	? () => void handleArchiveSession(summary)
																	: undefined
															}
															isPending={pendingSessionAction?.key === key}
															pendingAction={pendingAction}
															hideProjectLabel
															compact
															alwaysShowActions={alwaysShowRowActions}
														/>
													</li>
												);
											})}
										</ul>
									</CollapsibleContent>
								</Collapsible>
							);
						})}
					</div>
				)}
			</div>

			<footer className="relative flex items-center gap-3 border-t border-border/50 px-4 py-3.5 pb-[max(env(safe-area-inset-bottom),0.85rem)]">
				<a
					href={`https://github.com/${session.githubLogin}`}
					target="_blank"
					rel="noreferrer noopener"
					className="group flex min-w-0 flex-1 items-center gap-3 rounded-md px-1.5 py-1 -mx-1.5 -my-1 transition-colors hover:bg-accent/60"
				>
					<Avatar className="h-8 w-8 ring-1 ring-border/80 ring-offset-1 ring-offset-background">
						{session.avatarUrl ? (
							<AvatarImage src={session.avatarUrl} alt={session.githubLogin} />
						) : null}
						<AvatarFallback className="text-xs">
							{session.githubLogin.slice(0, 2).toUpperCase()}
						</AvatarFallback>
					</Avatar>
					<div className="flex min-w-0 flex-1 items-center gap-1">
						<span className="truncate text-[13px] font-medium text-foreground">
							{session.githubLogin}
						</span>
						<ArrowUpRight className="h-3 w-3 shrink-0 opacity-0 transition-opacity group-hover:opacity-60" />
					</div>
				</a>
				<div className="flex shrink-0 items-center gap-0.5">
					<InstallAppAction
						variant="compact"
						className="size-8 text-muted-foreground hover:text-foreground"
					/>
					<Button
						variant="ghost"
						size="icon"
						aria-label="Sign out"
						onClick={onSignOut}
						className="size-8 shrink-0 text-muted-foreground hover:text-destructive hover:bg-destructive/10"
					>
						<Power className="h-3.5 w-3.5" />
					</Button>
				</div>
			</footer>
		</>
	);
}

export { X as SessionListPanelCloseIcon };
