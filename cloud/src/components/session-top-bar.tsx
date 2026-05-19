import { Button } from "@xero/ui/components/ui/button";
import { Menu } from "lucide-react";
import type { ReactNode } from "react";

import { BrandLogo } from "#/components/brand-logo";
import { NewSessionPicker } from "#/components/new-session-picker";
import type { RemoteProjectSummary } from "#/lib/relay/session-store";

interface SessionTopBarProps {
	title: string;
	projects?: RemoteProjectSummary[];
	onSelectProject?: (projectId: string) => void;
	onPickerOpenChange?: (open: boolean) => void;
	drawerTrigger?: ReactNode;
}

export function SessionTopBar({
	title,
	projects = [],
	onSelectProject,
	onPickerOpenChange,
	drawerTrigger,
}: SessionTopBarProps) {
	return (
		<header className="sticky top-0 z-20 flex items-center justify-between gap-3 bg-background px-4 py-3">
			<div className="flex min-w-0 items-center gap-2">
				<BrandLogo className="size-4 shrink-0" aria-label="Xero" />
				<span aria-hidden="true" className="text-sm text-muted-foreground/50">
					|
				</span>
				<span
					className="truncate text-sm font-medium text-foreground"
					title={title}
				>
					{title}
				</span>
			</div>
			<div className="flex shrink-0 items-center gap-1">
				{onSelectProject ? (
					<NewSessionPicker
						projects={projects}
						onSelectProject={onSelectProject}
						onPickerOpenChange={onPickerOpenChange}
					/>
				) : null}
				{drawerTrigger ?? (
					<Button
						type="button"
						variant="ghost"
						size="icon"
						aria-label="Open sessions list"
						className="text-muted-foreground hover:text-foreground"
					>
						<Menu className="h-4 w-4" />
					</Button>
				)}
			</div>
		</header>
	);
}
