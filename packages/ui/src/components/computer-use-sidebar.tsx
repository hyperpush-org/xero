import { Monitor, X } from "lucide-react";
import {
	type ComponentPropsWithoutRef,
	type KeyboardEvent as ReactKeyboardEvent,
	type PointerEvent as ReactPointerEvent,
	type ReactNode,
	useCallback,
	useEffect,
	useRef,
	useState,
} from "react";

import { cn } from "../lib/utils";

export type ComputerUseSidebarDensity = "comfortable" | "compact";

export const COMPUTER_USE_SIDEBAR_MIN_WIDTH = 320;
export const COMPUTER_USE_SIDEBAR_MAX_WIDTH = 720;
export const COMPUTER_USE_SIDEBAR_DEFAULT_WIDTH = 560;
export const COMPUTER_USE_SIDEBAR_COMPACT_WIDTH = 400;

function clampSidebarWidth(width: number, minWidth: number, maxWidth: number) {
	return Math.max(minWidth, Math.min(maxWidth, width));
}

function readPersistedWidth(
	storageKey: string | undefined,
	minWidth: number,
	maxWidth: number,
): number | null {
	if (!storageKey || typeof window === "undefined") return null;
	try {
		const raw = window.localStorage?.getItem?.(storageKey);
		if (!raw) return null;
		const parsed = Number.parseInt(raw, 10);
		if (!Number.isFinite(parsed) || parsed < minWidth) return null;
		return clampSidebarWidth(parsed, minWidth, maxWidth);
	} catch {
		return null;
	}
}

function writePersistedWidth(
	storageKey: string | undefined,
	width: number,
): void {
	if (!storageKey || typeof window === "undefined") return;
	try {
		window.localStorage?.setItem?.(storageKey, String(Math.round(width)));
	} catch {
		/* Storage may be blocked; resizing should still work for this session. */
	}
}

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
	const resolvedMinWidth = Math.min(minWidth, maxWidth);
	const resolvedMaxWidth = Math.max(minWidth, maxWidth);
	const [width, setWidth] = useState(() =>
		clampSidebarWidth(
			readPersistedWidth(widthStorageKey, resolvedMinWidth, resolvedMaxWidth) ??
				defaultWidth,
			resolvedMinWidth,
			resolvedMaxWidth,
		),
	);
	const widthRef = useRef(width);
	const [isResizing, setIsResizing] = useState(false);
	const resizeCleanupRef = useRef<(() => void) | null>(null);
	widthRef.current = width;

	const density: ComputerUseSidebarDensity =
		width < compactWidth ? "compact" : "comfortable";

	useEffect(() => {
		if (!resizable) return;
		onDensityChange?.(density);
	}, [density, onDensityChange, resizable]);

	useEffect(() => {
		if (!resizable) return;
		onWidthChange?.(width);
	}, [onWidthChange, resizable, width]);

	useEffect(() => {
		if (!resizable) return;
		setWidth((current) =>
			clampSidebarWidth(current, resolvedMinWidth, resolvedMaxWidth),
		);
	}, [resizable, resolvedMaxWidth, resolvedMinWidth]);

	useEffect(() => {
		return () => {
			resizeCleanupRef.current?.();
			resizeCleanupRef.current = null;
		};
	}, []);

	const handleResizeStart = useCallback(
		(event: ReactPointerEvent<HTMLDivElement>) => {
			if (!resizable || event.button !== 0 || typeof window === "undefined") {
				return;
			}
			event.preventDefault();
			resizeCleanupRef.current?.();

			const startX = event.clientX;
			const startWidth = widthRef.current;
			let latestWidth = startWidth;
			let animationFrame: number | null = null;
			const previousCursor = document.body.style.cursor;
			const previousSelect = document.body.style.userSelect;

			const flushWidth = () => {
				animationFrame = null;
				setWidth(latestWidth);
			};
			const scheduleWidth = (nextWidth: number) => {
				latestWidth = nextWidth;
				if (typeof window.requestAnimationFrame !== "function") {
					setWidth(latestWidth);
					return;
				}
				if (animationFrame !== null) return;
				animationFrame = window.requestAnimationFrame(flushWidth);
			};
			const cancelFrame = () => {
				if (
					animationFrame !== null &&
					typeof window.cancelAnimationFrame === "function"
				) {
					window.cancelAnimationFrame(animationFrame);
				}
				animationFrame = null;
			};
			const handleMove = (moveEvent: PointerEvent) => {
				const delta = startX - moveEvent.clientX;
				scheduleWidth(
					clampSidebarWidth(
						startWidth + delta,
						resolvedMinWidth,
						resolvedMaxWidth,
					),
				);
			};
			const handleEnd = () => {
				cancelFrame();
				setWidth(latestWidth);
				writePersistedWidth(widthStorageKey, latestWidth);
				window.removeEventListener("pointermove", handleMove);
				window.removeEventListener("pointerup", handleEnd);
				window.removeEventListener("pointercancel", handleEnd);
				document.body.style.cursor = previousCursor;
				document.body.style.userSelect = previousSelect;
				setIsResizing(false);
				resizeCleanupRef.current = null;
			};

			document.body.style.cursor = "col-resize";
			document.body.style.userSelect = "none";
			setIsResizing(true);
			window.addEventListener("pointermove", handleMove);
			window.addEventListener("pointerup", handleEnd);
			window.addEventListener("pointercancel", handleEnd);
			resizeCleanupRef.current = handleEnd;
		},
		[resizable, resolvedMaxWidth, resolvedMinWidth, widthStorageKey],
	);

	const handleResizeKey = useCallback(
		(event: ReactKeyboardEvent<HTMLDivElement>) => {
			if (!resizable) return;
			if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
			event.preventDefault();
			const step = event.shiftKey ? 32 : 8;
			setWidth((current) => {
				const delta = event.key === "ArrowLeft" ? step : -step;
				const next = clampSidebarWidth(
					current + delta,
					resolvedMinWidth,
					resolvedMaxWidth,
				);
				writePersistedWidth(widthStorageKey, next);
				return next;
			});
		},
		[resizable, resolvedMaxWidth, resolvedMinWidth, widthStorageKey],
	);

	return (
		<aside
			{...props}
			aria-label={props["aria-label"] ?? "Computer Use agent"}
			className={cn(
				"relative flex h-dvh shrink-0 flex-col overflow-hidden border-l border-border/70 bg-sidebar",
				resizable ? null : "w-[min(31rem,34vw)] min-w-[22rem]",
				className,
			)}
			style={resizable ? { ...style, width } : style}
		>
			{resizable ? (
				<div
					aria-label={resizeLabel}
					aria-orientation="vertical"
					aria-valuemax={resolvedMaxWidth}
					aria-valuemin={resolvedMinWidth}
					aria-valuenow={width}
					className={cn(
						"absolute inset-y-0 -left-[3px] z-10 w-[6px] cursor-col-resize bg-transparent transition-colors",
						"hover:bg-primary/30",
						isResizing && "bg-primary/40",
					)}
					onKeyDown={handleResizeKey}
					onPointerDown={handleResizeStart}
					role="separator"
					tabIndex={0}
				/>
			) : null}
			{children}
		</aside>
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
