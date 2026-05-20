import { CheckIcon, ChevronDown, Cpu } from "lucide-react";
import { Fragment, type ReactNode, memo, useMemo, useState } from "react";
import { cn } from "../../lib/utils";
import {
	Command,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
	CommandSeparator,
} from "../ui/command";
import { Popover, PopoverContent, PopoverTrigger } from "../ui/popover";
import {
	ComposerInlineTrigger,
	composerInlineSelectContentClassName,
} from "./composer-inline-trigger";
import type { ComposerSelectGroup } from "./composer-types";

export interface ComposerModelSelectProps {
	groups: readonly ComposerSelectGroup[];
	value: string | null;
	onChange: (value: string) => void;
	disabled?: boolean;
	/** "pill" (inline toolbar) or "field" (full-width, for the settings menu). */
	variant?: "pill" | "field";
	triggerClassName?: string;
	icon?: ReactNode;
	ariaLabel?: string;
	placeholder?: string;
	searchPlaceholder?: string;
	emptyText?: string;
}

const fieldTriggerClassName =
	"flex h-9 w-full items-center justify-between gap-2 rounded-md border border-border/60 bg-background px-2.5 text-[13px] font-medium text-foreground shadow-none transition-colors hover:bg-muted/50 focus-visible:border-primary/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/15 data-[state=open]:border-primary/40 data-[state=open]:bg-muted/50 disabled:cursor-not-allowed disabled:opacity-50";

export const ComposerModelSelect = memo(function ComposerModelSelect({
	groups,
	value,
	onChange,
	disabled,
	variant = "pill",
	triggerClassName,
	icon,
	ariaLabel = "Model selector",
	placeholder = "Model not configured",
	searchPlaceholder = "Search models...",
	emptyText = "No models found.",
}: ComposerModelSelectProps) {
	const [open, setOpen] = useState(false);
	const selectedLabel = useMemo(() => {
		for (const group of groups) {
			const match = group.options.find((option) => option.id === value);
			if (match) return match.label;
		}
		return null;
	}, [groups, value]);

	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger asChild>
				{variant === "field" ? (
					<button
						type="button"
						role="combobox"
						aria-label={ariaLabel}
						aria-expanded={open}
						aria-haspopup="listbox"
						disabled={disabled}
						className={cn(fieldTriggerClassName, triggerClassName)}
					>
						<span className="line-clamp-1 truncate">
							{selectedLabel ?? placeholder}
						</span>
						<ChevronDown
							aria-hidden="true"
							className="size-3.5 text-muted-foreground/70"
						/>
					</button>
				) : (
					<ComposerInlineTrigger
						role="combobox"
						aria-label={ariaLabel}
						aria-expanded={open}
						aria-haspopup="listbox"
						className={triggerClassName}
						disabled={disabled}
						icon={icon ?? <Cpu aria-hidden="true" className="size-3" />}
						label={selectedLabel ?? placeholder}
					/>
				)}
			</PopoverTrigger>
			{open ? (
				<PopoverContent
					align="start"
					className={cn("w-72 p-0", composerInlineSelectContentClassName)}
				>
					<Command>
						<CommandInput placeholder={searchPlaceholder} />
						<CommandList>
							<CommandEmpty>{emptyText}</CommandEmpty>
							{groups.map((group, index) => (
								<Fragment key={group.id}>
									{index > 0 ? <CommandSeparator /> : null}
									<CommandGroup heading={group.label}>
										{group.options.map((option) => (
											<CommandItem
												key={option.id}
												value={`${group.label ?? ""} ${option.label}`}
												disabled={option.disabled}
												onSelect={() => {
													onChange(option.id);
													setOpen(false);
												}}
											>
												{option.icon ? (
													<span className="shrink-0">{option.icon}</span>
												) : null}
												<span className="line-clamp-1 truncate">
													{option.label}
												</span>
												{value === option.id ? (
													<CheckIcon
														aria-hidden="true"
														className="ml-auto size-3.5"
													/>
												) : null}
											</CommandItem>
										))}
									</CommandGroup>
								</Fragment>
							))}
						</CommandList>
					</Command>
				</PopoverContent>
			) : null}
		</Popover>
	);
});
