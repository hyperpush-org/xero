import { Button } from "@xero/ui/components/ui/button";
import { ChevronRight, Menu } from "lucide-react";
import type { ReactNode } from "react";

import { BrandLogo } from "#/components/brand-logo";

interface SessionTopBarProps {
	title: string;
	projectLabel?: string;
	drawerTrigger?: ReactNode;
}

export function SessionTopBar({
	title,
	projectLabel,
	drawerTrigger,
}: SessionTopBarProps) {
	return (
		<header className="sticky top-0 z-20 flex items-center justify-between gap-1.5 bg-background px-3.5 lg:px-5 pb-2 pt-[max(env(safe-area-inset-top),0.5rem)] lg:py-4">
			<span
				aria-hidden
				className="pointer-events-none absolute inset-x-0 -top-6 h-20 opacity-50"
				style={{
					background:
						"radial-gradient(60% 100% at 50% 0%, var(--cloud-halo-soft), transparent 75%)",
				}}
			/>
			<div className="relative flex min-w-0 items-center gap-1.5 text-[12.5px] text-muted-foreground">
				<BrandLogo className="size-3.5 shrink-0 lg:hidden" aria-label="Xero" />
				<ChevronRight
					aria-hidden="true"
					className="h-3 w-3 shrink-0 text-muted-foreground/70 lg:hidden"
				/>
				{projectLabel ? (
					<span className="hidden min-w-0 items-center gap-1.5 lg:flex">
						<span
							className="truncate font-semibold text-foreground"
							title={projectLabel}
						>
							{projectLabel}
						</span>
						<ChevronRight
							aria-hidden="true"
							className="h-3 w-3 shrink-0 text-muted-foreground/70"
						/>
					</span>
				) : null}
				<span className="truncate font-medium" title={title}>
					{title}
				</span>
			</div>
			<div className="relative flex shrink-0 items-center gap-1">
				{drawerTrigger ?? (
					<Button
						type="button"
						variant="ghost"
						size="icon"
						aria-label="Open sessions list"
						className="text-muted-foreground hover:text-foreground lg:hidden"
					>
						<Menu className="h-4 w-4" />
					</Button>
				)}
			</div>
		</header>
	);
}
