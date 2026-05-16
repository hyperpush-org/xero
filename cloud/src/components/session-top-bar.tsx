import { Button } from "@xero/ui/components/ui/button";
import { Menu, Plus } from "lucide-react";
import type { ReactNode } from "react";

interface SessionTopBarProps {
	title: string;
	onNewSession?: () => void;
	drawerTrigger?: ReactNode;
}

export function SessionTopBar({
	title,
	onNewSession,
	drawerTrigger,
}: SessionTopBarProps) {
	return (
		<header className="sticky top-0 z-20 flex items-center justify-between gap-2 border-b border-border bg-background/95 px-4 py-3 backdrop-blur supports-[backdrop-filter]:bg-background/75">
			<h1 className="truncate text-base font-medium" title={title}>
				{title}
			</h1>
			<div className="flex shrink-0 items-center gap-1">
				{onNewSession ? (
					<Button
						type="button"
						variant="ghost"
						size="icon"
						aria-label="Start new session"
						onClick={onNewSession}
					>
						<Plus className="h-5 w-5" />
					</Button>
				) : null}
				{drawerTrigger ?? (
					<Button variant="ghost" size="icon" aria-label="Open sessions list">
						<Menu className="h-5 w-5" />
					</Button>
				)}
			</div>
		</header>
	);
}
