import { cn } from "@xero/ui/lib/utils";
import { Monitor } from "lucide-react";

import type { VisibleSessionSummary } from "#/lib/relay/session-store";

interface SessionListRowProps {
	summary: VisibleSessionSummary;
	isActive: boolean;
	onSelect: () => void;
}

export function SessionListRow({
	summary,
	isActive,
	onSelect,
}: SessionListRowProps) {
	const subtitle = summary.computerName ?? "Desktop";
	return (
		<button
			type="button"
			onClick={onSelect}
			className={cn(
				"flex w-full items-start gap-3 rounded-md px-3 py-2.5 text-left transition-colors",
				isActive ? "bg-accent text-accent-foreground" : "hover:bg-accent/50",
			)}
		>
			<Monitor className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
			<div className="flex flex-1 flex-col gap-0.5 min-w-0">
				<p className="truncate text-sm font-medium">
					{summary.title || "Untitled session"}
				</p>
				<p className="truncate text-[11.5px] text-muted-foreground">
					{subtitle}
				</p>
			</div>
		</button>
	);
}
