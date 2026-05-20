import { ChevronDown } from "lucide-react";
import {
	type ComponentPropsWithoutRef,
	type ReactNode,
	forwardRef,
} from "react";
import { cn } from "../../lib/utils";

export const composerInlineTriggerClassName =
	"flex h-7 w-fit min-w-0 items-center gap-1 rounded-md border-0 bg-transparent px-2 text-[12px] font-medium text-muted-foreground/90 whitespace-nowrap shadow-none transition-colors outline-none hover:bg-muted/60 hover:text-foreground focus-visible:border-transparent focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-50 data-[state=open]:bg-muted/60 data-[state=open]:text-foreground dark:bg-transparent dark:hover:bg-muted/60 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg]:text-muted-foreground/70";

export const composerInlineSelectContentClassName =
	"max-h-72 min-w-44 border-border/70 bg-card/95 text-foreground shadow-xl backdrop-blur supports-[backdrop-filter]:bg-card/90 [&_[data-slot=select-item]]:px-2.5 [&_[data-slot=select-item]]:pr-9";

export interface ComposerInlineTriggerProps
	extends ComponentPropsWithoutRef<"button"> {
	icon: ReactNode;
	label: ReactNode;
}

export const ComposerInlineTrigger = forwardRef<
	HTMLButtonElement,
	ComposerInlineTriggerProps
>(function ComposerInlineTrigger(
	{ icon, label, className, type, ...props },
	ref,
) {
	return (
		<button
			ref={ref}
			type={type ?? "button"}
			className={cn(composerInlineTriggerClassName, className)}
			{...props}
		>
			{icon}
			<span className="line-clamp-1 truncate">{label}</span>
			<ChevronDown aria-hidden="true" className="size-4 opacity-50" />
		</button>
	);
});
