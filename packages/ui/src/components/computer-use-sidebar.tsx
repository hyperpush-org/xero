import { Monitor, X } from "lucide-react";
import {
	type ComponentPropsWithoutRef,
	type ReactNode,
} from "react";

import { cn } from "../lib/utils";
import {
	ResizableSidebar,
	type ResizableSidebarDensity,
} from "./resizable-sidebar";

export type ComputerUseSidebarDensity = ResizableSidebarDensity;

export const COMPUTER_USE_SIDEBAR_MIN_WIDTH = 320;
export const COMPUTER_USE_SIDEBAR_MAX_WIDTH = 720;
export const COMPUTER_USE_SIDEBAR_DEFAULT_WIDTH = 560;
export const COMPUTER_USE_SIDEBAR_COMPACT_WIDTH = 400;

export interface ComputerUseSidebarProps
	extends ComponentPropsWithoutRef<"aside"> {
	compactWidth?: number;
	defaultWidth?: number;
	maxWidth?: number;
	minWidth?: number;
	onDensityChange?: (density: ComputerUseSidebarDensity) => void;
	onWidthChange?: (width: number) => void;
	resizable?: boolean;
	resizeLabel?: string;
	widthStorageKey?: string;
}

export function ComputerUseSidebar({
	children,
	className,
	compactWidth = COMPUTER_USE_SIDEBAR_COMPACT_WIDTH,
	defaultWidth = COMPUTER_USE_SIDEBAR_DEFAULT_WIDTH,
	maxWidth = COMPUTER_USE_SIDEBAR_MAX_WIDTH,
	minWidth = COMPUTER_USE_SIDEBAR_MIN_WIDTH,
	onDensityChange,
	onWidthChange,
	resizable = false,
	resizeLabel = "Resize Computer Use sidebar",
	style,
	widthStorageKey,
	...props
}: ComputerUseSidebarProps) {
	return (
		<ResizableSidebar
			{...props}
			aria-label={props["aria-label"] ?? "Computer Use agent"}
			compactWidth={compactWidth}
			defaultWidth={defaultWidth}
			maxWidth={maxWidth}
			minWidth={minWidth}
			nonResizableClassName="w-[min(31rem,34vw)] min-w-[22rem]"
			onDensityChange={onDensityChange}
			onWidthChange={onWidthChange}
			resizable={resizable}
			resizeEdge="left"
			resizeLabel={resizeLabel}
			widthStorageKey={widthStorageKey}
			className={cn("border-l border-border/70", className)}
			style={style}
		>
			{children}
		</ResizableSidebar>
	);
}

export interface ComputerUseSidebarHeaderProps
	extends ComponentPropsWithoutRef<"div"> {
	closeLabel?: string;
	label?: ReactNode;
	onClose?: () => void;
}

export function ComputerUseSidebarHeader({
	className,
	closeLabel = "Close Computer Use",
	label = "Computer Use",
	onClose,
	...props
}: ComputerUseSidebarHeaderProps) {
	return (
		<div
			{...props}
			className={cn(
				"flex h-10 shrink-0 items-center justify-between gap-1.5 bg-sidebar px-3.5",
				className,
			)}
		>
			<div className="flex min-w-0 items-center gap-2 text-[12.5px]">
				<span className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
					<Monitor className="h-3.5 w-3.5" aria-hidden="true" />
				</span>
				<h2 className="truncate font-semibold text-foreground">{label}</h2>
			</div>
			{onClose ? (
				<button
					type="button"
					aria-label={closeLabel}
					onClick={onClose}
					className="inline-flex h-[30px] w-[30px] items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
				>
					<X className="h-3.5 w-3.5" aria-hidden="true" />
				</button>
			) : null}
		</div>
	);
}

export interface ComputerUseSidebarContentProps
	extends ComponentPropsWithoutRef<"div"> {}

export function ComputerUseSidebarContent({
	children,
	className,
	...props
}: ComputerUseSidebarContentProps) {
	return (
		<div {...props} className={cn("flex min-h-0 flex-1 flex-col", className)}>
			{children}
		</div>
	);
}
