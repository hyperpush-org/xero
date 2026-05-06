"use client"

import { useCallback, useRef } from "react"
import { Crosshair, ExternalLink, Search, X } from "lucide-react"
import { cn } from "@/lib/utils"
import type { ElementInfo, UseInspector } from "@/src/features/emulator/use-inspector"

interface InspectorOverlayProps {
  /** Device dimensions (in device pixels). */
  deviceWidth: number
  deviceHeight: number
  /** The inspector state from useInspector(). */
  inspector: UseInspector
  /** Called when the user clicks an element to open source (RN only). */
  onOpenSource?: (file: string, line: number, column: number) => void
  /** Called to search the project for a given AX identifier or label. */
  onSearchProject?: (query: string) => void
}

/**
 * Transparent overlay rendered on top of the emulator frame when inspect
 * mode is active. Captures ALL pointer events (suppresses touch input to
 * the device). Queries element-at-point via Metro or AXUIElement, renders
 * highlight rectangle and info tooltip.
 */
export function InspectorOverlay({
  deviceWidth,
  deviceHeight,
  inspector,
  onOpenSource,
  onSearchProject,
}: InspectorOverlayProps) {
  const overlayRef = useRef<HTMLDivElement>(null)

  // Convert pointer position to device pixels.
  const toDeviceCoords = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      const rect = e.currentTarget.getBoundingClientRect()
      if (rect.width === 0 || rect.height === 0) return null
      const nx = (e.clientX - rect.left) / rect.width
      const ny = (e.clientY - rect.top) / rect.height
      return {
        x: Math.round(nx * deviceWidth),
        y: Math.round(ny * deviceHeight),
      }
    },
    [deviceWidth, deviceHeight],
  )

  const handlePointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      // Suppress event propagation so the device doesn't receive touch input.
      e.stopPropagation()
      e.preventDefault()
      const coords = toDeviceCoords(e)
      if (!coords) return
      inspector.elementAt(coords.x, coords.y)
    },
    [toDeviceCoords, inspector],
  )

  const handlePointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      // Suppress all pointer events — inspect mode owns the viewport.
      e.stopPropagation()
      e.preventDefault()
    },
    [],
  )

  const handleClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      e.stopPropagation()
      e.preventDefault()
      const el = inspector.hoveredElement
      if (!el) return

      // RN apps: open source file.
      if (el.source && onOpenSource) {
        onOpenSource(el.source.file, el.source.line, el.source.column)
        return
      }

      // Native apps: search project for AX identifier or label.
      const searchTerm = el.componentName || el.nativeType
      if (searchTerm && onSearchProject) {
        onSearchProject(searchTerm)
      }
    },
    [inspector.hoveredElement, onOpenSource, onSearchProject],
  )

  const el = inspector.hoveredElement
  const isNativeMode = el != null && el.source == null

  return (
    <div
      ref={overlayRef}
      className="absolute inset-0 z-20 cursor-crosshair"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerDown}
      onClick={handleClick}
    >
      {/* Highlight rectangle */}
      {el && deviceWidth > 0 && (
        <HighlightBox
          bounds={el.bounds}
          deviceWidth={deviceWidth}
          deviceHeight={deviceHeight}
        />
      )}

      {/* Tooltip */}
      {el && (
        <ElementTooltip
          element={el}
          hasSource={!!onOpenSource}
          hasSearch={!!onSearchProject}
          isNativeMode={isNativeMode}
        />
      )}

      {/* Inspect mode badge */}
      <div className="absolute left-2 top-2 flex items-center gap-1.5">
        <div className="flex items-center gap-1 rounded-md bg-primary/90 px-1.5 py-0.5 text-[10px] font-medium text-primary-foreground shadow-sm">
          <Crosshair className="h-3 w-3" />
          Inspect
        </div>
        {isNativeMode && (
          <div className="rounded-md bg-muted/90 px-1.5 py-0.5 text-[9px] text-muted-foreground shadow-sm">
            Accessibility view — source correlation is best-effort
          </div>
        )}
      </div>
    </div>
  )
}

// MARK: - Subcomponents

function HighlightBox({
  bounds,
  deviceWidth,
  deviceHeight,
}: {
  bounds: { x: number; y: number; w: number; h: number }
  deviceWidth: number
  deviceHeight: number
}) {
  // Convert device coords to percentage-based positioning.
  const left = `${(bounds.x / deviceWidth) * 100}%`
  const top = `${(bounds.y / deviceHeight) * 100}%`
  const width = `${(bounds.w / deviceWidth) * 100}%`
  const height = `${(bounds.h / deviceHeight) * 100}%`

  return (
    <div
      className="pointer-events-none absolute border-2 border-primary/80 bg-primary/10"
      style={{ left, top, width, height }}
    />
  )
}

function ElementTooltip({
  element,
  hasSource,
  hasSearch,
  isNativeMode,
}: {
  element: ElementInfo
  hasSource: boolean
  hasSearch: boolean
  isNativeMode: boolean
}) {
  return (
    <div className="absolute bottom-2 left-2 right-2 flex flex-col gap-0.5 rounded-md border border-border/60 bg-popover/95 px-2 py-1.5 text-[10px] shadow-md backdrop-blur-sm">
      {/* Element name + native type */}
      <div className="flex items-center gap-1.5">
        <span className="font-semibold text-foreground">
          {isNativeMode ? (
            // Native: show AX role/type
            <>{element.nativeType || "Unknown"}</>
          ) : (
            // RN: show component name
            <>{"<"}{element.componentName || "Unknown"}{" />"}</>
          )}
        </span>
        {!isNativeMode && element.nativeType && (
          <span className="text-muted-foreground">({element.nativeType})</span>
        )}
      </div>

      {/* Label (for native AX elements) */}
      {isNativeMode && element.componentName && element.componentName !== element.nativeType && (
        <div className="text-foreground/80">
          Label: &ldquo;{element.componentName}&rdquo;
        </div>
      )}

      {/* Bounds */}
      <div className="text-muted-foreground">
        {element.bounds.w}×{element.bounds.h} at ({element.bounds.x}, {element.bounds.y})
      </div>

      {/* Source location (RN only) */}
      {element.source && (
        <div className="flex items-center gap-1 text-primary">
          <ExternalLink className="h-2.5 w-2.5" />
          <span className="truncate">
            {element.source.file.split("/").pop()}:{element.source.line}
          </span>
          {hasSource && (
            <span className="text-muted-foreground/60">(click to open)</span>
          )}
        </div>
      )}

      {/* Search project (native AX — best-effort source correlation) */}
      {isNativeMode && hasSearch && (element.componentName || element.nativeType) && (
        <div className="flex items-center gap-1 text-primary">
          <Search className="h-2.5 w-2.5" />
          <span className="truncate">
            Search project for &ldquo;{element.componentName || element.nativeType}&rdquo;
          </span>
          <span className="text-muted-foreground/60">(click)</span>
        </div>
      )}
    </div>
  )
}

// MARK: - Inspect mode toggle button (for use in toolbar)

export function InspectModeButton({
  active,
  connected,
  disabled,
  onClick,
}: {
  active: boolean
  connected: boolean
  disabled?: boolean
  onClick: () => void
}) {
  return (
    <button
      aria-label={active ? "Exit inspect mode" : "Enter inspect mode"}
      aria-pressed={active}
      className={cn(
        "inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-[11px] transition-colors",
        active
          ? "border-primary bg-primary/20 text-primary"
          : "border-border/70 bg-background/60 text-foreground hover:border-primary/50 hover:text-primary",
        disabled && "opacity-50 cursor-not-allowed",
      )}
      disabled={disabled}
      onClick={onClick}
      title={connected ? "Inspect React Native elements" : "Inspect UI elements (Accessibility)"}
      type="button"
    >
      <Crosshair className="h-3 w-3" />
      {active ? "Inspecting" : "Inspect"}
    </button>
  )
}
