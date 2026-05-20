import { Button } from "@xero/ui/components/ui/button";
import {
	Drawer,
	DrawerContent,
	DrawerHeader,
	DrawerTitle,
	DrawerTrigger,
} from "@xero/ui/components/ui/drawer";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@xero/ui/components/ui/dropdown-menu";
import { useIsMobile } from "@xero/ui/components/ui/use-mobile";
import { ChevronRight, FolderGit2, Loader2, Plus } from "lucide-react";
import { useState } from "react";

import type { RemoteProjectSummary } from "#/lib/relay/session-store";

interface NewSessionPickerProps {
	projects: RemoteProjectSummary[];
	onSelectProject: (project: RemoteProjectSummary) => void;
	/** Called when the picker's open state changes. Use on mobile to close any covering sidebar. */
	onPickerOpenChange?: (open: boolean) => void;
	disabledHint?: string;
	pendingProjectKey?: string | null;
}

const DEFAULT_DISABLED_HINT =
	"No projects available. Open Xero on your desktop to add one.";

export function NewSessionPicker({
	projects,
	onSelectProject,
	onPickerOpenChange,
	disabledHint,
	pendingProjectKey = null,
}: NewSessionPickerProps) {
	const isMobile = useIsMobile();
	const [pickerOpen, setPickerOpen] = useState(false);
	const projectCount = projects.length;
	const isDisabled = projectCount === 0;
	const hasPendingProject = Boolean(pendingProjectKey);
	const triggerLabel = isDisabled
		? (disabledHint ?? DEFAULT_DISABLED_HINT)
		: "Start new session";

	if (isDisabled) {
		return (
			<Button
				type="button"
				variant="ghost"
				size="icon"
				aria-label={triggerLabel}
				title={triggerLabel}
				disabled
				className="text-muted-foreground"
			>
				<Plus className="h-4 w-4" />
			</Button>
		);
	}

	if (projectCount === 1) {
		const only = projects[0];
		const isPending = pendingProjectKey === projectKey(only);
		return (
			<Button
				type="button"
				variant="ghost"
				size="icon"
				aria-label={`Start new session in ${only.projectName ?? only.projectId}`}
				onClick={() => onSelectProject(only)}
				disabled={hasPendingProject}
				className="text-muted-foreground hover:text-foreground"
			>
				{isPending ? (
					<Loader2 className="h-4 w-4 animate-spin" />
				) : (
					<Plus className="h-4 w-4" />
				)}
			</Button>
		);
	}

	const handleOpenChange = (open: boolean) => {
		setPickerOpen(open);
		onPickerOpenChange?.(open);
	};

	const handleSelect = (project: RemoteProjectSummary) => {
		handleOpenChange(false);
		onSelectProject(project);
	};

	const triggerButton = (
		<Button
			type="button"
			variant="ghost"
			size="icon"
			aria-label={triggerLabel}
			disabled={hasPendingProject}
			className="text-muted-foreground hover:text-foreground"
		>
			{hasPendingProject ? (
				<Loader2 className="h-4 w-4 animate-spin" />
			) : (
				<Plus className="h-4 w-4" />
			)}
		</Button>
	);

	if (isMobile) {
		return (
			<Drawer open={pickerOpen} onOpenChange={handleOpenChange}>
				<DrawerTrigger asChild>{triggerButton}</DrawerTrigger>
				<DrawerContent className="data-[vaul-drawer-direction=bottom]:rounded-t-3xl border-t border-border/60 px-1.5 pb-[max(env(safe-area-inset-bottom),0.75rem)]">
					<DrawerHeader className="px-3 pt-1 pb-3.5 text-left">
						<div className="flex items-baseline justify-between gap-3">
							<DrawerTitle className="font-display text-[18px] font-medium tracking-tight">
								New{" "}
								<em className="font-display-italic text-primary">session</em>
							</DrawerTitle>
							<span className="text-cloud-meta tabular-nums text-muted-foreground/70">
								{projectCount} {projectCount === 1 ? "project" : "projects"}
							</span>
						</div>
					</DrawerHeader>
					<div className="flex max-h-[60vh] flex-col overflow-y-auto px-1.5 pb-1.5">
						{projects.map((project) => (
							<button
								key={projectKey(project)}
								type="button"
								onClick={() => handleSelect(project)}
								disabled={hasPendingProject}
								className="group flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors hover:bg-accent active:bg-accent/70"
							>
								<span className="flex size-7 shrink-0 items-center justify-center rounded-md bg-muted/60 text-muted-foreground transition-colors group-hover:bg-background group-hover:text-foreground">
									{pendingProjectKey === projectKey(project) ? (
										<Loader2 className="h-3.5 w-3.5 animate-spin" />
									) : (
										<FolderGit2 className="h-3.5 w-3.5" />
									)}
								</span>
								<span className="min-w-0 flex-1 truncate text-[13px] font-medium text-foreground">
									{project.projectName ?? project.projectId}
								</span>
								<ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground/50 transition-colors group-hover:text-foreground" />
							</button>
						))}
					</div>
				</DrawerContent>
			</Drawer>
		);
	}

	return (
		<DropdownMenu open={pickerOpen} onOpenChange={handleOpenChange}>
			<DropdownMenuTrigger asChild>{triggerButton}</DropdownMenuTrigger>
			<DropdownMenuContent align="end" className="min-w-[16rem]">
				<DropdownMenuLabel className="font-display-italic text-[12.5px] font-normal text-muted-foreground">
					New session in…
				</DropdownMenuLabel>
				<DropdownMenuSeparator />
				{projects.map((project) => (
					<DropdownMenuItem
						key={projectKey(project)}
						onSelect={() => handleSelect(project)}
						disabled={hasPendingProject}
						className="gap-2"
					>
						{pendingProjectKey === projectKey(project) ? (
							<Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
						) : (
							<FolderGit2 className="h-4 w-4 text-muted-foreground" />
						)}
						<span className="truncate font-medium">
							{project.projectName ?? project.projectId}
						</span>
					</DropdownMenuItem>
				))}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

function projectKey(project: RemoteProjectSummary): string {
	return `${project.computerId}:${project.projectId}`;
}
