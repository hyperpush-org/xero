import {
	ArrowUp,
	LoaderCircle,
	Mic,
	Plus,
	Sparkles,
	Square,
} from "lucide-react";
import { cn } from "../../lib/utils";
import { Button } from "../ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip";
import type { ComposerDictationControl } from "./use-composer-dictation";

export type ComposerActionDensity = "sm" | "md";

function actionSizeClass(density: ComposerActionDensity): string {
	return density === "md" ? "h-8 w-8" : "h-7 w-7";
}

function actionIconSize(_density: ComposerActionDensity): string {
	return "h-3.5 w-3.5";
}

interface ComposerAttachButtonProps {
	density?: ComposerActionDensity;
	className?: string;
	disabled?: boolean;
	onClick: () => void;
}

export function ComposerAttachButton({
	density = "sm",
	className,
	disabled,
	onClick,
}: ComposerAttachButtonProps) {
	const iconSize = actionIconSize(density);
	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					type="button"
					variant="ghost"
					size="icon-sm"
					className={cn(
						actionSizeClass(density),
						"rounded-md text-muted-foreground/80 hover:text-foreground",
						className,
					)}
					aria-label="Add files"
					disabled={disabled}
					onClick={onClick}
				>
					<Plus className={iconSize} strokeWidth={2.25} />
				</Button>
			</TooltipTrigger>
			<TooltipContent side="top">Add files</TooltipContent>
		</Tooltip>
	);
}

interface ComposerMicButtonProps {
	density?: ComposerActionDensity;
	className?: string;
	dictation: Pick<
		ComposerDictationControl,
		"ariaLabel" | "isListening" | "isToggleDisabled" | "phase" | "toggle" | "tooltip"
	>;
}

export function ComposerMicButton({
	density = "sm",
	className,
	dictation,
}: ComposerMicButtonProps) {
	const iconSize = actionIconSize(density);
	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					type="button"
					variant={dictation.isListening ? "outline" : "ghost"}
					size="icon-sm"
					className={cn(
						actionSizeClass(density),
						"relative rounded-md px-0 text-muted-foreground/70 hover:text-foreground",
						dictation.isListening
							? "border-destructive/35 bg-destructive/10 text-destructive hover:bg-destructive/15 hover:text-destructive"
							: null,
						className,
					)}
					disabled={dictation.isToggleDisabled}
					aria-label={dictation.ariaLabel}
					aria-pressed={dictation.isListening}
					onClick={() => void dictation.toggle()}
				>
					{dictation.phase === "requesting" || dictation.phase === "stopping" ? (
						<LoaderCircle className={cn(iconSize, "animate-spin")} strokeWidth={2.25} />
					) : (
						<Mic
							className={cn(iconSize, dictation.isListening ? "animate-pulse" : null)}
							strokeWidth={2.25}
						/>
					)}
				</Button>
			</TooltipTrigger>
			<TooltipContent side="top">{dictation.tooltip}</TooltipContent>
		</Tooltip>
	);
}

interface ComposerSendButtonProps {
	density?: ComposerActionDensity;
	className?: string;
	disabled: boolean;
	isLoading?: boolean;
	onClick: () => void;
	ariaLabel?: string;
	showKbdHint?: boolean;
}

export function ComposerSendButton({
	density = "sm",
	className,
	disabled,
	isLoading,
	onClick,
	ariaLabel = "Send message",
	showKbdHint = false,
}: ComposerSendButtonProps) {
	const iconSize = actionIconSize(density);
	const button = (
		<Button
			type="button"
			size="icon-sm"
			variant="secondary"
			className={cn(
				actionSizeClass(density),
				"agent-send-button rounded-md px-0",
				className,
			)}
			disabled={disabled}
			onClick={onClick}
			aria-label={ariaLabel}
		>
			{isLoading ? (
				<LoaderCircle className={cn(iconSize, "animate-spin")} strokeWidth={2.5} />
			) : (
				<ArrowUp data-send-icon className={iconSize} strokeWidth={2.5} />
			)}
		</Button>
	);
	if (!showKbdHint) {
		return button;
	}
	return (
		<Tooltip>
			<TooltipTrigger asChild>{button}</TooltipTrigger>
			<TooltipContent side="top" className="flex items-center gap-1.5">
				<span>{ariaLabel}</span>
				<kbd className="rounded border border-border/60 bg-foreground/10 px-1 py-0.5 font-sans text-[10px] leading-none">
					⏎
				</kbd>
			</TooltipContent>
		</Tooltip>
	);
}

interface ComposerStopButtonProps {
	density?: ComposerActionDensity;
	className?: string;
	disabled: boolean;
	isLoading?: boolean;
	onClick: () => void;
	ariaLabel?: string;
}

export function ComposerStopButton({
	density = "sm",
	className,
	disabled,
	isLoading,
	onClick,
	ariaLabel = "Stop agent run",
}: ComposerStopButtonProps) {
	const iconSize = actionIconSize(density);
	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					type="button"
					size="icon-sm"
					variant="secondary"
					className={cn(actionSizeClass(density), "rounded-md px-0", className)}
					disabled={disabled}
					onClick={onClick}
					aria-label={ariaLabel}
				>
					{isLoading ? (
						<LoaderCircle className={cn(iconSize, "animate-spin")} strokeWidth={2.5} />
					) : (
						<Square className={cn(iconSize, "fill-current")} strokeWidth={2.5} />
					)}
				</Button>
			</TooltipTrigger>
			<TooltipContent side="top">{ariaLabel}</TooltipContent>
		</Tooltip>
	);
}

interface ComposerAutoCompactToggleProps {
	density?: ComposerActionDensity;
	className?: string;
	enabled: boolean;
	disabled?: boolean;
	onChange: (next: boolean) => void;
}

export function ComposerAutoCompactToggle({
	density = "sm",
	className,
	enabled,
	disabled,
	onChange,
}: ComposerAutoCompactToggleProps) {
	const iconSize = actionIconSize(density);
	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					type="button"
					size="icon-sm"
					variant="ghost"
					aria-label="Auto-compact before sending"
					aria-pressed={enabled}
					className={cn(
						actionSizeClass(density),
						"rounded-md px-0 text-muted-foreground/70 hover:text-foreground",
						enabled
							? "bg-primary/10 text-primary hover:bg-primary/15 hover:text-primary"
							: null,
						className,
					)}
					disabled={disabled}
					onClick={() => onChange(!enabled)}
				>
					<Sparkles className={iconSize} strokeWidth={2.5} />
				</Button>
			</TooltipTrigger>
			<TooltipContent side="top">
				Auto-compact before sending {enabled ? "· on" : "· off"}
			</TooltipContent>
		</Tooltip>
	);
}

