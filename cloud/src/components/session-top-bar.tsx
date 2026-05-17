import { AppLogo } from "@xero/ui/components/app-logo";
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
		<header className="sticky top-0 z-20 flex items-center justify-between gap-3 bg-background px-5 py-4">
			<div className="flex min-w-0 items-center gap-2.5">
				<AppLogo className="size-6 shrink-0" aria-label="Xero" />
				<span
					className="truncate text-base font-medium text-foreground/90"
					title={title}
				>
					{title}
				</span>
			</div>
			<div className="flex shrink-0 items-center gap-1">
				{onNewSession ? (
					<Button
						type="button"
						variant="ghost"
						size="icon"
						aria-label="Start new session"
						onClick={onNewSession}
						className="size-10 text-muted-foreground hover:text-foreground [&_svg]:size-[22px]"
					>
						<Plus />
					</Button>
				) : null}
				{drawerTrigger ?? (
					<Button
						type="button"
						variant="ghost"
						size="icon"
						aria-label="Open sessions list"
						className="size-10 text-muted-foreground hover:text-foreground [&_svg]:size-[22px]"
					>
						<Menu />
					</Button>
				)}
			</div>
		</header>
	);
}
