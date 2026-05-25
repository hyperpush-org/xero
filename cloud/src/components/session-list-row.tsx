import { Button } from "@xero/ui/components/ui/button";
import { cn } from "@xero/ui/lib/utils";
import { Archive, Loader2, Monitor } from "lucide-react";
import { type FocusEvent, useEffect, useRef, useState } from "react";

import type { VisibleSessionSummary } from "#/lib/relay/session-store";

const ARCHIVE_CONFIRMATION_TIMEOUT_MS = 4000;

interface SessionListRowProps {
	summary: VisibleSessionSummary;
	isActive: boolean;
	onSelect: () => void;
	onArchive?: () => void;
	isPending?: boolean;
	pendingAction?: "archive";
	hideProjectLabel?: boolean;
	compact?: boolean;
	alwaysShowActions?: boolean;
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
	onArchive,
	isPending = false,
	pendingAction,
	hideProjectLabel = false,
	compact = false,
	alwaysShowActions = false,
}: SessionListRowProps) {
	const timeLabel = compact ? null : formatRelativeTime(summary.lastActivityAt);
	const title = summary.title || "Untitled session";
	const projectLabel =
		compact || hideProjectLabel
			? null
			: (summary.projectName ?? summary.projectId);

	const titleBlock = (
		<div className="flex min-w-0 flex-1 flex-col">
			<span className="flex min-w-0 items-center gap-1.5">
				{summary.isComputerUse ? (
					<Monitor className="size-3.5 shrink-0 text-primary" />
				) : null}
				<span
					className={cn(
						"truncate leading-tight",
						compact ? "text-[12.5px]" : "text-[13px]",
						isActive
							? "font-medium text-foreground"
							: "font-normal text-foreground/90",
					)}
				>
					{title}
				</span>
			</span>
			{projectLabel || timeLabel ? (
				<span className="mt-1 flex items-center gap-1.5 truncate text-[11px] leading-tight text-muted-foreground/75">
					{projectLabel ? (
						<span className="truncate font-medium tracking-[0.04em]">
							{projectLabel}
						</span>
					) : null}
					{projectLabel && timeLabel ? (
						<span aria-hidden className="text-muted-foreground/40">
							·
						</span>
					) : null}
					{timeLabel ? (
						<span className="font-display-italic shrink-0 tracking-normal text-muted-foreground/70">
							{timeLabel}
						</span>
					) : null}
				</span>
			) : null}
		</div>
	);

	const container = cn(
		"group relative flex items-center transition-colors",
		compact
			? cn("mx-2 rounded-md", isActive ? "bg-accent/50" : "hover:bg-accent/30")
			: cn("w-full", isActive ? "bg-primary/[0.07]" : "hover:bg-accent/40"),
	);
	const selectButtonClassName = cn(
		"flex min-w-0 flex-1 items-center text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-60",
		compact ? "rounded-md px-3 py-2" : "px-4 py-3",
	);
	const actionIconClassName = compact ? "size-3.5" : "size-3.5";
	const isArchivePending = isPending && pendingAction === "archive";

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
				"inline-flex shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-40",
				compact ? "h-6" : "h-7",
				isArchiveConfirming
					? cn(
							"w-auto bg-destructive/10 font-semibold text-destructive hover:bg-destructive/15",
							compact
								? "min-w-[54px] px-1.5 text-[10px]"
								: "min-w-[62px] px-2 text-[11px]",
						)
					: compact
						? "w-6"
						: "w-7",
			)}
		>
			{isArchivePending ? (
				<Loader2 className={cn("animate-spin", actionIconClassName)} />
			) : isArchiveConfirming ? (
				<span>Archive</span>
			) : (
				<Archive className={actionIconClassName} />
			)}
		</Button>
	) : null;

	return (
		<div className={container}>
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
			{archiveButton ? (
				<div
					className={cn(
						"flex shrink-0 items-center transition-opacity",
						compact ? "mr-2 gap-1.5" : "mr-2 gap-1",
						isPending
							? "opacity-100"
							: isActive || alwaysShowActions
								? "opacity-70"
								: "opacity-0 group-hover:opacity-70 group-focus-within:opacity-70",
					)}
				>
					{archiveButton}
				</div>
			) : null}
		</div>
	);
}
