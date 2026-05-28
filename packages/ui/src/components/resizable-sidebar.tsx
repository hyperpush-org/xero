import {
	type ComponentPropsWithoutRef,
	type KeyboardEvent as ReactKeyboardEvent,
	type PointerEvent as ReactPointerEvent,
	useCallback,
	useEffect,
	useRef,
	useState,
} from "react";

import { cn } from "../lib/utils";

export type ResizableSidebarDensity = "comfortable" | "compact";
export type ResizableSidebarResizeEdge = "left" | "right";

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

export interface ResizableSidebarProps extends ComponentPropsWithoutRef<"aside"> {
	compactWidth?: number;
	defaultWidth: number;
	maxWidth: number;
	minWidth: number;
	nonResizableClassName?: string;
	onDensityChange?: (density: ResizableSidebarDensity) => void;
	onWidthChange?: (width: number) => void;
	resizable?: boolean;
	resizeEdge?: ResizableSidebarResizeEdge;
	resizeLabel?: string;
	widthStorageKey?: string;
}

export function ResizableSidebar({
	children,
	className,
	compactWidth,
	defaultWidth,
	maxWidth,
	minWidth,
	nonResizableClassName,
	onDensityChange,
	onWidthChange,
	resizable = false,
	resizeEdge = "left",
	resizeLabel = "Resize sidebar",
	style,
	widthStorageKey,
	...props
}: ResizableSidebarProps) {
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

	const density: ResizableSidebarDensity =
		compactWidth && width < compactWidth ? "compact" : "comfortable";

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
				const pointerDelta = moveEvent.clientX - startX;
				const widthDelta =
					resizeEdge === "right" ? pointerDelta : -pointerDelta;
				scheduleWidth(
					clampSidebarWidth(
						startWidth + widthDelta,
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
		[resizable, resizeEdge, resolvedMaxWidth, resolvedMinWidth, widthStorageKey],
	);

	const handleResizeKey = useCallback(
		(event: ReactKeyboardEvent<HTMLDivElement>) => {
			if (!resizable) return;
			if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
			event.preventDefault();
			const step = event.shiftKey ? 32 : 8;
			const directionalStep = event.key === "ArrowRight" ? step : -step;
			setWidth((current) => {
				const widthDelta =
					resizeEdge === "right" ? directionalStep : -directionalStep;
				const next = clampSidebarWidth(
					current + widthDelta,
					resolvedMinWidth,
					resolvedMaxWidth,
				);
				writePersistedWidth(widthStorageKey, next);
				return next;
			});
		},
		[resizable, resizeEdge, resolvedMaxWidth, resolvedMinWidth, widthStorageKey],
	);

	return (
		<aside
			{...props}
			className={cn(
				"relative flex h-dvh shrink-0 flex-col overflow-hidden bg-sidebar",
				resizable ? null : nonResizableClassName,
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
						"absolute inset-y-0 z-10 w-[6px] cursor-col-resize bg-transparent transition-colors",
						resizeEdge === "right" ? "-right-[3px]" : "-left-[3px]",
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
