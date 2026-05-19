import { Button } from "@xero/ui/components/ui/button";
import { cn } from "@xero/ui/lib/utils";
import { Archive, Loader2, Unlink2 } from "lucide-react";
import { type FocusEvent, useEffect, useRef, useState } from "react";

import type { VisibleSessionSummary } from "#/lib/relay/session-store";

const ARCHIVE_CONFIRMATION_TIMEOUT_MS = 4000;

interface SessionListRowProps {
	summary: VisibleSessionSummary;
	isActive: boolean;
	onSelect: () => void;
	onSetRemoteVisibility?: (visible: boolean) => void;
	onArchive?: () => void;
	isPending?: boolean;
	pendingAction?: "visibility" | "archive";
}

function formatRelativeTime(iso: string | null): string | null {
	if (!iso) return null;
	const then = new Date(iso).getTime();
	if (Number.isNaN(then)) return null;
	const diffSec = Math.max(0, Math.round((Date.now() - then) / 1000));
	if (diffSec < 45) return "just now";
	const diffMin = Math.round(diffSec / 60);
	if (diffMin < 60) return `${diffMin}m ago`;
	const diffHr = Math.round(diffMin / 60);
	if (diffHr < 24) return `${diffHr}h ago`;
	const diffDay = Math.round(diffHr / 24);
	if (diffDay < 7) return `${diffDay}d ago`;
	return new Date(iso).toLocaleDateString(undefined, {
		month: "short",
		day: "numeric",
	});
}

export function SessionListRow({
	summary,
	isActive,
	onSelect,
	onSetRemoteVisibility,
	onArchive,
	isPending = false,
	pendingAction,
}: SessionListRowProps) {
	const timeLabel = formatRelativeTime(summary.lastActivityAt);
	const isLinked = summary.remoteVisible;
	const title = summary.title || "Untitled session";
	const projectLabel = summary.projectName ?? summary.projectId;

	const metaParts = [projectLabel, timeLabel].filter(Boolean);
	const meta = metaParts.join(" · ");

	const titleBlock = (
		<div className="flex min-w-0 flex-1 flex-col">
			<span
				className={cn(
					"truncate text-[13px] leading-tight",
					isActive
						? "font-medium text-foreground"
						: "font-normal text-foreground/90",
				)}
			>
				{title}
			</span>
			{meta ? (
				<span className="mt-0.5 truncate text-[11px] leading-tight text-muted-foreground/80">
					{meta}
				</span>
			) : null}
		</div>
	);

	const canSelect = isLinked || Boolean(onSetRemoteVisibility);
	const container = cn(
		"group relative flex w-full items-center transition-colors",
		isActive
			? "bg-accent/70"
			: isLinked
				? "hover:bg-accent/40"
				: "hover:bg-accent/30",
	);
	const selectButtonClassName =
		"flex min-w-0 flex-1 items-center px-4 py-3 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-60";
	const actionGroupClassName = cn(
		"mr-2 flex shrink-0 items-center gap-1 transition-opacity",
		isActive || isPending
			? "opacity-100"
			: "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
	);
	const actionButtonClassName =
		"inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-background hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-40";
	const isArchivePending = isPending && pendingAction === "archive";
	const isVisibilityPending = isPending && pendingAction !== "archive";

	const [archiveConfirmationSessionId, setArchiveConfirmationSessionId] =
		useState<string | null>(null);
	const archiveButtonRef = useRef<HTMLButtonElement | null>(null);
	const archiveConfirmationActive =
		archiveConfirmationSessionId === summary.sessionId;
	useEffect(() => {
		if (isPending || !onArchive) {
			setArchiveConfirmationSessionId(null);
		}
	}, [isPending, onArchive]);
	useEffect(() => {
		if (!archiveConfirmationActive) return;
		const handlePointerDown = (event: PointerEvent) => {
			const target = event.target;
			if (
				target instanceof Node &&
				archiveButtonRef.current?.contains(target)
			) {
				return;
			}
			setArchiveConfirmationSessionId(null);
		};
		const timeoutId = window.setTimeout(
			() => setArchiveConfirmationSessionId(null),
			ARCHIVE_CONFIRMATION_TIMEOUT_MS,
		);
		document.addEventListener("pointerdown", handlePointerDown, true);
		return () => {
			window.clearTimeout(timeoutId);
			document.removeEventListener("pointerdown", handlePointerDown, true);
		};
	}, [archiveConfirmationActive]);
	const isArchiveConfirming =
		archiveConfirmationActive && Boolean(onArchive) && !isPending;
	const clearArchiveConfirmation = () => setArchiveConfirmationSessionId(null);

	const archiveButton = onArchive ? (
		<Button
			ref={archiveButtonRef}
			type="button"
			variant="ghost"
			size="icon"
			onClick={() => {
				if (isArchiveConfirming) {
					setArchiveConfirmationSessionId(null);
					onArchive();
					return;
				}
				setArchiveConfirmationSessionId(summary.sessionId);
			}}
			onBlur={(event: FocusEvent<HTMLButtonElement>) => {
				const nextFocused = event.relatedTarget;
				if (
					nextFocused instanceof Node &&
					event.currentTarget.contains(nextFocused)
				) {
					return;
				}
				clearArchiveConfirmation();
			}}
			onKeyDown={(event) => {
				if (event.key === "Escape") {
					event.stopPropagation();
					clearArchiveConfirmation();
				}
			}}
			disabled={isPending}
			aria-label={
				isArchiveConfirming ? `Confirm archive ${title}` : `Archive ${title}`
			}
			title={
				isArchiveConfirming
					? `Press again to archive ${title}`
					: `Archive ${title}`
			}
			className={cn(
				"inline-flex h-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-40",
				isArchiveConfirming
					? "w-auto min-w-[62px] bg-destructive/10 px-2 text-[11px] font-semibold text-destructive hover:bg-destructive/15"
					: "w-7",
			)}
		>
			{isArchivePending ? (
				<Loader2 className="h-3.5 w-3.5 animate-spin" />
			) : isArchiveConfirming ? (
				<span>Archive</span>
			) : (
				<Archive className="h-3.5 w-3.5" />
			)}
		</Button>
	) : null;

	if (!isLinked) {
		return (
			<div className={container}>
				<button
					type="button"
					onClick={onSelect}
					disabled={!canSelect || isPending}
					aria-label={`Open ${title}`}
					className={selectButtonClassName}
				>
					{titleBlock}
				</button>
				{archiveButton || isVisibilityPending ? (
					<div className={actionGroupClassName}>
						{isVisibilityPending ? (
							<Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin text-muted-foreground" />
						) : null}
						{archiveButton}
					</div>
				) : null}
			</div>
		);
	}

	return (
		<div className={container}>
			{isActive ? (
				<span
					aria-hidden
					className="pointer-events-none absolute inset-y-0 left-0 w-0.5 bg-primary"
				/>
			) : null}
			<button
				type="button"
				onClick={onSelect}
				disabled={isPending}
				aria-label={`Open ${title}`}
				aria-current={isActive ? "page" : undefined}
				className={selectButtonClassName}
			>
				{titleBlock}
			</button>
			<div
				className={cn(
					"mr-2 flex shrink-0 items-center gap-1 transition-opacity",
					isPending
						? "opacity-100"
						: isActive
							? "opacity-70"
							: "opacity-0 group-hover:opacity-70 group-focus-within:opacity-70",
				)}
			>
				{archiveButton}
				<Button
					type="button"
					variant="ghost"
					size="icon"
					onClick={() => onSetRemoteVisibility?.(false)}
					disabled={!onSetRemoteVisibility || isPending}
					aria-label={`Unlink ${title}`}
					className={actionButtonClassName}
				>
					{isVisibilityPending ? (
						<Loader2 className="h-3.5 w-3.5 animate-spin" />
					) : (
						<Unlink2 className="h-3.5 w-3.5" />
					)}
				</Button>
			</div>
		</div>
	);
}
