import { cn } from "@xero/ui/lib/utils";
import { ChevronRight, GitBranch, Lightbulb, Search } from "lucide-react";

import { BrandLogo } from "#/components/brand-logo";

interface Suggestion {
	icon: typeof Search;
	label: string;
	prompt: string;
}

const SUGGESTIONS: Suggestion[] = [
	{
		icon: Search,
		label: "Explore the codebase",
		prompt:
			"Walk me through this codebase: structure, key modules, and where to start reading.",
	},
	{
		icon: GitBranch,
		label: "Review recent commits",
		prompt:
			"Review my recent commits for correctness risks and maintainability concerns.",
	},
	{
		icon: Lightbulb,
		label: "Suggest next steps",
		prompt:
			"Based on this project, suggest a few high-impact things I could work on next.",
	},
];

interface EmptySessionStateProps {
	projectLabel: string;
	onSelectSuggestion?: (prompt: string) => void;
}

export function EmptySessionState({
	projectLabel,
	onSelectSuggestion,
}: EmptySessionStateProps) {
	return (
		<div className="relative flex w-full flex-1 items-center justify-center overflow-hidden">
			<div className="relative flex w-full max-w-md flex-col items-center px-6 py-6 text-center sm:max-w-xl sm:px-8 sm:py-12">
				<div className="flex h-10 w-10 items-center justify-center rounded-2xl border border-border bg-card/60 sm:h-12 sm:w-12">
					<BrandLogo className="size-5 sm:size-7" aria-label="Xero" />
				</div>

				<h2 className="mt-4 text-lg font-semibold leading-snug tracking-tight text-foreground sm:mt-5 sm:text-2xl md:text-[26px]">
					What can we build together in{" "}
					<span className="text-primary">{projectLabel}</span>?
				</h2>
				<p className="mt-2 max-w-sm text-[12.5px] leading-relaxed text-muted-foreground sm:mt-3 sm:max-w-md sm:text-[13px]">
					Just ask — I can read your code, suggest changes, or run a task for
					you. Everything we do will show up right here.
				</p>

				{onSelectSuggestion ? (
					<ul className="mt-6 flex w-full max-w-sm flex-col divide-y divide-border/60 overflow-hidden rounded-xl border border-border/70 bg-card/40 backdrop-blur-sm sm:mt-8 sm:max-w-md">
						{SUGGESTIONS.map((suggestion) => (
							<li key={suggestion.label}>
								<button
									type="button"
									onClick={() => onSelectSuggestion(suggestion.prompt)}
									className={cn(
										"group flex w-full items-center gap-3 px-3.5 py-2.5 text-left transition-colors sm:px-4 sm:py-3",
										"hover:bg-secondary/40 focus-visible:bg-secondary/40 focus-visible:outline-none",
									)}
								>
									<suggestion.icon
										className="h-3.5 w-3.5 shrink-0 text-muted-foreground group-hover:text-primary"
										aria-hidden="true"
									/>
									<span className="flex-1 truncate text-[12.5px] text-foreground/85 group-hover:text-foreground sm:text-[13px]">
										{suggestion.label}
									</span>
									<ChevronRight
										aria-hidden="true"
										className="h-3.5 w-3.5 shrink-0 text-muted-foreground/70"
									/>
								</button>
							</li>
						))}
					</ul>
				) : null}
			</div>
		</div>
	);
}
