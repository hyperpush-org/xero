import { cn } from "@xero/ui/lib/utils";
import { Monitor } from "lucide-react";

import type { VisibleSessionSummary } from "#/lib/relay/session-store";

interface SessionListRowProps {
	summary: VisibleSessionSummary;
	isActive: boolean;
	showComputer?: boolean;
	onSelect: () => void;
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
	showComputer = true,
	onSelect,
}: SessionListRowProps) {
	const computer = summary.computerName ?? "Desktop";
	const timeLabel = formatRelativeTime(summary.lastActivityAt);

	return (
		<button
			type="button"
			onClick={onSelect}
			aria-current={isActive ? "page" : undefined}
			className={cn(
				"group relative flex w-full items-start gap-3 rounded-lg px-3 py-3 text-left transition-colors",
				"focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
				isActive
					? "bg-accent/80 text-accent-foreground"
					: "hover:bg-accent/40 active:bg-accent/60",
			)}
		>
			<span
				aria-hidden
				className={cn(
					"absolute left-0 top-2 bottom-2 w-0.5 rounded-full transition-colors",
					isActive ? "bg-primary" : "bg-transparent",
				)}
			/>
			<span
				className={cn(
					"mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-md border transition-colors",
					isActive
						? "border-primary/30 bg-primary/10 text-primary"
						: "border-border/70 bg-secondary/40 text-muted-foreground group-hover:text-foreground",
				)}
			>
				<Monitor className="h-4 w-4" />
			</span>
			<div className="flex flex-1 min-w-0 flex-col gap-0.5">
				<p className="truncate text-[13.5px] font-medium leading-tight">
					{summary.title || "Untitled session"}
				</p>
				<div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
					{showComputer ? (
						<>
							<span className="truncate">{computer}</span>
							{timeLabel ? (
								<span aria-hidden className="opacity-50">
									·
								</span>
							) : null}
						</>
					) : null}
					{timeLabel ? (
						<span className="shrink-0 tabular-nums">{timeLabel}</span>
					) : null}
				</div>
			</div>
		</button>
	);
}
