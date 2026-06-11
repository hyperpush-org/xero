import { Brain, CheckIcon, ChevronDown, Cpu } from "lucide-react";
import {
	Fragment,
	type ReactNode,
	type WheelEvent,
	memo,
	useCallback,
	useMemo,
	useRef,
	useState,
} from "react";
import { cn } from "../lib/utils";
import {
	Command,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
	CommandSeparator,
} from "./ui/command";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuRadioGroup,
	DropdownMenuRadioItem,
	DropdownMenuSub,
	DropdownMenuSubContent,
	DropdownMenuSubTrigger,
	DropdownMenuTrigger,
} from "./ui/dropdown-menu";
import {
	ComposerInlineTrigger,
	composerInlineSelectContentClassName,
} from "./composer/composer-inline-trigger";
import type {
	ComposerSelectGroup,
	ComposerSelectOption,
} from "./composer/composer-types";

export type ModelThinkingSelectOption = ComposerSelectOption;
export type ModelThinkingSelectGroup = ComposerSelectGroup;

export interface ModelThinkingSelectProps {
	groups: readonly ModelThinkingSelectGroup[];
	value: string | null;
	onChange: (value: string) => void;
	disabled?: boolean;
	thinkingOptions?: readonly ModelThinkingSelectOption[];
	selectedThinkingId?: string | null;
	onThinkingChange?: (value: string) => void;
	thinkingDisabled?: boolean;
	thinkingPlaceholder?: string;
	thinkingLabel?: string;
	/** "pill" (inline toolbar) or "field" (full-width, for dialogs/settings). */
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

export const ModelThinkingSelect = memo(function ModelThinkingSelect({
	groups,
	value,
	onChange,
	disabled,
	thinkingOptions,
	selectedThinkingId,
	onThinkingChange,
	thinkingDisabled,
	thinkingPlaceholder = "Thinking",
	thinkingLabel = "Thinking",
	variant = "pill",
	triggerClassName,
	icon,
	ariaLabel = onThinkingChange ? "Model and thinking selector" : "Model selector",
	placeholder = "Model not configured",
	searchPlaceholder = "Search models...",
	emptyText = "No models found.",
}: ModelThinkingSelectProps) {
	const [open, setOpen] = useState(false);
	const listRef = useRef<HTMLDivElement | null>(null);
	const selectedLabel = useMemo(() => {
		for (const group of groups) {
			const match = group.options.find((option) => option.id === value);
			if (match) return match.label;
		}
		return null;
	}, [groups, value]);
	const selectedThinkingLabel = useMemo(
		() =>
			thinkingOptions?.find((option) => option.id === selectedThinkingId)
				?.label ?? null,
		[thinkingOptions, selectedThinkingId],
	);
	const hasThinkingOptions = Boolean(thinkingOptions && thinkingOptions.length > 0);
	const showThinking = typeof onThinkingChange === "function";
	const triggerLabel =
		showThinking && selectedThinkingLabel ? (
			<span className="flex min-w-0 max-w-full flex-1 items-center gap-1.5">
				<span className="min-w-0 max-w-full truncate">
					{selectedLabel ?? placeholder}
				</span>
				<span className="text-muted-foreground/45">·</span>
				<span className="shrink-0 text-muted-foreground/75">
					{selectedThinkingLabel}
				</span>
			</span>
		) : (
			selectedLabel ?? placeholder
		);
	const thinkingControlDisabled =
		Boolean(thinkingDisabled) || !hasThinkingOptions || !onThinkingChange;

	const thinkingMenu =
		showThinking && hasThinkingOptions && thinkingOptions ? (
			<DropdownMenuSub>
				<DropdownMenuSubTrigger
					disabled={thinkingControlDisabled}
					className="mx-1 h-8 gap-1.5 rounded-md px-2 text-[12px]"
				>
					<Brain aria-hidden="true" className="size-3.5" />
					<span className="min-w-0 flex-1 truncate">{thinkingLabel}</span>
					<span className="truncate text-muted-foreground/75">
						{selectedThinkingLabel ?? thinkingPlaceholder}
					</span>
				</DropdownMenuSubTrigger>
				<DropdownMenuSubContent
					sideOffset={6}
					className="min-w-36 border-border/70 bg-card/95 text-foreground shadow-xl backdrop-blur supports-[backdrop-filter]:bg-card/90"
				>
					<DropdownMenuRadioGroup
						value={selectedThinkingId ?? undefined}
						onValueChange={(nextValue) => {
							if (!nextValue) return;
							onThinkingChange?.(nextValue);
							setOpen(false);
						}}
					>
						{thinkingOptions.map((option) => (
							<DropdownMenuRadioItem
								key={option.id}
								value={option.id}
								disabled={thinkingControlDisabled || option.disabled}
								className="text-[12px]"
							>
								{option.label}
							</DropdownMenuRadioItem>
						))}
					</DropdownMenuRadioGroup>
				</DropdownMenuSubContent>
			</DropdownMenuSub>
		) : showThinking ? (
			<DropdownMenuItem
				disabled
				className="mx-1 h-8 gap-1.5 rounded-md px-2 text-[12px]"
			>
				<Brain aria-hidden="true" className="size-3.5" />
				<span className="min-w-0 flex-1 truncate">{thinkingLabel}</span>
				<span className="truncate text-muted-foreground/75">
					{thinkingPlaceholder}
				</span>
			</DropdownMenuItem>
		) : null;

	const handleWheelCapture = useCallback((event: WheelEvent<HTMLDivElement>) => {
		const list = listRef.current;
		if (!list) return;

		const deltaY =
			event.deltaMode === 1
				? event.deltaY * 16
				: event.deltaMode === 2
					? event.deltaY * list.clientHeight
					: event.deltaY;
		if (deltaY === 0) return;

		const maxScrollTop = list.scrollHeight - list.clientHeight;
		if (maxScrollTop <= 0) return;

		const nextScrollTop = Math.max(
			0,
			Math.min(maxScrollTop, list.scrollTop + deltaY),
		);
		if (nextScrollTop === list.scrollTop) return;

		event.preventDefault();
		event.stopPropagation();
		list.scrollTop = nextScrollTop;
	}, []);

	return (
		<DropdownMenu open={open} onOpenChange={setOpen} modal={false}>
			<DropdownMenuTrigger asChild>
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
						<span className="flex min-w-0 flex-1 items-center overflow-hidden text-left">
							{triggerLabel}
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
						label={triggerLabel}
					/>
				)}
			</DropdownMenuTrigger>
			{open ? (
				<DropdownMenuContent
					align="start"
					className={cn(
						composerInlineSelectContentClassName,
						"max-h-none w-72 overflow-visible p-0",
					)}
					onWheelCapture={handleWheelCapture}
				>
					<Command>
						<CommandInput placeholder={searchPlaceholder} />
						{thinkingMenu ? (
							<div className="border-b border-border/60 py-1">
								{thinkingMenu}
							</div>
						) : null}
						<CommandList
							ref={listRef}
							className="max-h-[min(18rem,calc(var(--radix-dropdown-menu-content-available-height)_-_5rem))]"
						>
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
													if (!showThinking) {
														setOpen(false);
													}
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
				</DropdownMenuContent>
			) : null}
		</DropdownMenu>
	);
});
