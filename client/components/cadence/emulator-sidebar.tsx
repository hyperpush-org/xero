"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  Apple,
  Loader2,
  Play,
  RotateCcw,
  Square,
  Smartphone,
  X,
} from "lucide-react"
import { cn } from "@/lib/utils"
import {
  useEmulatorSession,
  type EmulatorInputKind,
  type EmulatorOrientation,
  type EmulatorPlatform,
} from "@/src/features/emulator/use-emulator-session"
import { EmulatorHardwareStrip } from "./emulator-hardware-strip"
import { EmulatorMissingSdk } from "./emulator-missing-sdk"

interface EmulatorSidebarProps {
  open: boolean
  platform: EmulatorPlatform
}

const MIN_WIDTH = 320
const RIGHT_PADDING = 200
const DEFAULT_RATIO = 0.4
// Matches BrowserSidebar — keeps the left-edge drag handle clickable once the
// frame viewport paints content over the sidebar.
const RESIZE_HANDLE_INSET = 6

const PLATFORM_META: Record<EmulatorPlatform, {
  label: string
  storageKey: string
  ariaResize: string
}> = {
  ios: {
    label: "iOS Simulator",
    storageKey: "cadence.emulator.ios.width",
    ariaResize: "Resize iOS simulator sidebar",
  },
  android: {
    label: "Android Emulator",
    storageKey: "cadence.emulator.android.width",
    ariaResize: "Resize Android emulator sidebar",
  },
}

function viewportDefaultWidth() {
  if (typeof window === "undefined") return 640
  return Math.round(window.innerWidth * DEFAULT_RATIO)
}

function viewportMaxWidth() {
  if (typeof window === "undefined") return 1600
  return Math.max(MIN_WIDTH, window.innerWidth - RIGHT_PADDING)
}

function readPersistedWidth(storageKey: string): number | null {
  if (typeof window === "undefined") return null
  try {
    const raw = window.localStorage?.getItem?.(storageKey)
    if (!raw) return null
    const parsed = Number.parseInt(raw, 10)
    if (!Number.isFinite(parsed) || parsed < MIN_WIDTH) return null
    return parsed
  } catch {
    return null
  }
}

function writePersistedWidth(storageKey: string, width: number): void {
  if (typeof window === "undefined") return
  try {
    window.localStorage?.setItem?.(storageKey, String(Math.round(width)))
  } catch {
    /* storage quota / privacy mode — width falls back to default next session */
  }
}

export function EmulatorSidebar({ open, platform }: EmulatorSidebarProps) {
  const meta = PLATFORM_META[platform]
  const [width, setWidth] = useState(() =>
    readPersistedWidth(meta.storageKey) ?? viewportDefaultWidth(),
  )
  const [maxWidth, setMaxWidth] = useState(viewportMaxWidth)
  const [isResizing, setIsResizing] = useState(false)
  const widthRef = useRef(width)
  widthRef.current = width
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null)

  const session = useEmulatorSession({ platform, active: open })

  useEffect(() => {
    if (typeof window === "undefined") return
    const handleResize = () => {
      const nextMax = viewportMaxWidth()
      setMaxWidth(nextMax)
      setWidth((current) => Math.min(current, nextMax))
    }
    window.addEventListener("resize", handleResize)
    return () => window.removeEventListener("resize", handleResize)
  }, [])

  useEffect(() => {
    writePersistedWidth(meta.storageKey, width)
  }, [meta.storageKey, width])

  useEffect(() => {
    // Default-select the first device when the list hydrates, or refresh the
    // selection if the previous one disappeared (e.g. AVD deleted).
    if (session.devices.length === 0) {
      setSelectedDeviceId(null)
      return
    }
    setSelectedDeviceId((current) => {
      if (current && session.devices.some((d) => d.id === current)) return current
      return session.devices[0].id
    })
  }, [session.devices])

  const handleResizeStart = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return
      event.preventDefault()
      const startX = event.clientX
      const startWidth = widthRef.current
      const ceiling = viewportMaxWidth()
      setMaxWidth(ceiling)
      setIsResizing(true)

      const previousCursor = document.body.style.cursor
      const previousSelect = document.body.style.userSelect
      document.body.style.cursor = "col-resize"
      document.body.style.userSelect = "none"

      const handleMove = (ev: PointerEvent) => {
        const delta = startX - ev.clientX
        const next = Math.max(MIN_WIDTH, Math.min(ceiling, startWidth + delta))
        setWidth(next)
      }
      const handleUp = () => {
        window.removeEventListener("pointermove", handleMove)
        window.removeEventListener("pointerup", handleUp)
        window.removeEventListener("pointercancel", handleUp)
        document.body.style.cursor = previousCursor
        document.body.style.userSelect = previousSelect
        setIsResizing(false)
      }

      window.addEventListener("pointermove", handleMove)
      window.addEventListener("pointerup", handleUp)
      window.addEventListener("pointercancel", handleUp)
    },
    [],
  )

  const handleResizeKey = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return
    event.preventDefault()
    const step = event.shiftKey ? 32 : 8
    const ceiling = viewportMaxWidth()
    setMaxWidth(ceiling)
    setWidth((current) => {
      const delta = event.key === "ArrowLeft" ? step : -step
      return Math.max(MIN_WIDTH, Math.min(ceiling, current + delta))
    })
  }, [])

  const Icon = platform === "ios" ? Apple : Smartphone

  const handleStart = useCallback(() => {
    if (!selectedDeviceId) return
    void session.start(selectedDeviceId)
  }, [selectedDeviceId, session])

  const handleStop = useCallback(() => {
    void session.stop()
  }, [session])

  const isStreaming = session.status.phase === "streaming"
  const isActive =
    session.status.phase === "streaming" ||
    session.status.phase === "booting" ||
    session.status.phase === "connecting"

  const handleRotate = useCallback(() => {
    const next: EmulatorOrientation =
      session.orientation === "portrait" ? "landscape" : "portrait"
    void session.rotate(next)
  }, [session])

  const handlePressKey = useCallback(
    (key: string) => {
      void session.pressKey(key)
    },
    [session],
  )

  // Keyboard capture: when the viewport is focused, route printable chars to
  // the device as text, arrow keys / modifiers as hardware keys, and let
  // Cmd/Ctrl combinations pass through to Cadence so the user doesn't lose
  // global shortcuts (Cmd+W, Cmd+S, etc.).
  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (!isStreaming) return
      if (event.metaKey || event.ctrlKey) return

      const key = event.key
      if (key === "Enter") {
        event.preventDefault()
        void session.pressKey("enter")
        return
      }
      if (key === "Backspace") {
        event.preventDefault()
        void session.pressKey("backspace")
        return
      }
      if (key === "Tab") {
        event.preventDefault()
        void session.pressKey("tab")
        return
      }
      if (key === "Escape") {
        event.preventDefault()
        void session.pressKey("escape")
        return
      }
      if (key === "ArrowLeft" || key === "ArrowRight" || key === "ArrowUp" || key === "ArrowDown") {
        event.preventDefault()
        const mapped =
          key === "ArrowLeft"
            ? "dpad_left"
            : key === "ArrowRight"
              ? "dpad_right"
              : key === "ArrowUp"
                ? "dpad_up"
                : "dpad_down"
        void session.pressKey(mapped)
        return
      }
      if (key.length === 1) {
        event.preventDefault()
        void session.sendInput({ kind: "text", text: key })
      }
    },
    [isStreaming, session],
  )

  return (
    <aside
      aria-hidden={!open}
      aria-label={meta.label}
      className={cn(
        "relative flex shrink-0 flex-col overflow-hidden border-l border-border/80 bg-sidebar",
        !isResizing && "transition-[width] duration-200 ease-out",
        !open && "border-l-0",
      )}
      inert={!open ? true : undefined}
      style={{ width: open ? width : 0 }}
    >
      <div
        aria-label={meta.ariaResize}
        aria-orientation="vertical"
        aria-valuemax={maxWidth}
        aria-valuemin={MIN_WIDTH}
        aria-valuenow={width}
        className={cn(
          "absolute inset-y-0 -left-[3px] z-10 w-[6px] cursor-col-resize bg-transparent transition-colors",
          "hover:bg-primary/30",
          isResizing && "bg-primary/40",
        )}
        onKeyDown={handleResizeKey}
        onPointerDown={handleResizeStart}
        role="separator"
        tabIndex={open ? 0 : -1}
      />

      <div
        className="flex h-10 shrink-0 items-center gap-2 border-b border-border/70 px-3"
        style={{ paddingLeft: RESIZE_HANDLE_INSET + 6 }}
      >
        <Icon className="h-3.5 w-3.5 text-muted-foreground" />
        <span className="text-[11px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
          {meta.label}
        </span>
        <div className="ml-auto flex items-center gap-1">
          {session.devices.length > 0 ? (
            <select
              aria-label="Device"
              className="h-6 rounded-md border border-border/70 bg-background/40 px-1.5 text-[11px] text-foreground focus:border-primary/50 focus:outline-none disabled:opacity-60"
              disabled={isActive}
              onChange={(e) => setSelectedDeviceId(e.target.value)}
              value={selectedDeviceId ?? ""}
            >
              {session.devices.map((device) => (
                <option key={device.id} value={device.id}>
                  {device.displayName}
                </option>
              ))}
            </select>
          ) : null}
          {isActive ? (
            <>
              <button
                aria-label={`Rotate ${session.orientation === "portrait" ? "to landscape" : "to portrait"}`}
                className="flex h-6 w-6 items-center justify-center rounded-md border border-border/70 bg-background/40 text-muted-foreground transition-colors hover:border-primary/50 hover:text-primary disabled:opacity-60"
                disabled={!isStreaming}
                onClick={handleRotate}
                title="Rotate"
                type="button"
              >
                <RotateCcw className="h-3 w-3" />
              </button>
              <button
                aria-label="Stop device"
                className="flex h-6 items-center gap-1 rounded-md border border-border/70 bg-background/60 px-2 text-[11px] text-foreground transition-colors hover:border-destructive/40 hover:text-destructive disabled:opacity-60"
                disabled={session.isStopping}
                onClick={handleStop}
                type="button"
              >
                {session.isStopping ? (
                  <Loader2 className="h-3 w-3 animate-spin" />
                ) : (
                  <Square className="h-3 w-3" />
                )}
                Stop
              </button>
            </>
          ) : (
            <button
              aria-label="Start device"
              className="flex h-6 items-center gap-1 rounded-md border border-border/70 bg-background/60 px-2 text-[11px] text-foreground transition-colors hover:border-primary/40 hover:text-primary disabled:opacity-60"
              disabled={!selectedDeviceId || session.isStarting}
              onClick={handleStart}
              type="button"
            >
              {session.isStarting ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : (
                <Play className="h-3 w-3" />
              )}
              Start
            </button>
          )}
        </div>
      </div>

      <EmulatorMissingSdk platform={platform} />

      <EmulatorViewport
        currentDevice={session.currentDevice}
        error={session.error}
        frameSeq={session.frame?.seq ?? null}
        inputError={session.inputError}
        isStreaming={isStreaming}
        onDismissInputError={session.dismissInputError}
        onInput={(payload) => void session.sendInput(payload)}
        onKeyDown={handleKeyDown}
        orientation={session.orientation}
        platform={platform}
        platformLabel={meta.label}
        status={session.status}
      />

      <EmulatorHardwareStrip
        disabled={!isStreaming}
        onPressKey={handlePressKey}
        platform={platform}
      />
    </aside>
  )
}

interface ViewportProps {
  currentDevice: ReturnType<typeof useEmulatorSession>["currentDevice"]
  error: string | null
  frameSeq: number | null
  inputError: string | null
  isStreaming: boolean
  onDismissInputError: () => void
  onInput: (input: { kind: EmulatorInputKind; x?: number; y?: number }) => void
  onKeyDown: (event: React.KeyboardEvent<HTMLDivElement>) => void
  orientation: EmulatorOrientation
  platform: EmulatorPlatform
  platformLabel: string
  status: ReturnType<typeof useEmulatorSession>["status"]
}

function EmulatorViewport({
  currentDevice,
  error,
  frameSeq,
  inputError,
  isStreaming,
  onDismissInputError,
  onInput,
  onKeyDown,
  orientation,
  platform,
  platformLabel,
  status,
}: ViewportProps) {
  const imgRef = useRef<HTMLImageElement | null>(null)

  const frameSrc = useMemo(() => {
    if (frameSeq === null) return null
    return `emulator://localhost/frame?t=${frameSeq}`
  }, [frameSeq])

  const toNormalized = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const node = event.currentTarget
    const rect = node.getBoundingClientRect()
    if (rect.width === 0 || rect.height === 0) return null
    const x = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width))
    const y = Math.min(1, Math.max(0, (event.clientY - rect.top) / rect.height))
    return { x, y }
  }, [])

  const handlePointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (!isStreaming) return
      const coords = toNormalized(event)
      if (!coords) return
      event.currentTarget.setPointerCapture(event.pointerId)
      onInput({ kind: "touch_down", x: coords.x, y: coords.y })
    },
    [isStreaming, onInput, toNormalized],
  )

  const handlePointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (!isStreaming) return
      if (!(event.buttons & 1)) return
      const coords = toNormalized(event)
      if (!coords) return
      onInput({ kind: "touch_move", x: coords.x, y: coords.y })
    },
    [isStreaming, onInput, toNormalized],
  )

  const handlePointerUp = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (!isStreaming) return
      event.currentTarget.releasePointerCapture(event.pointerId)
      const coords = toNormalized(event)
      onInput({
        kind: "touch_up",
        x: coords?.x,
        y: coords?.y,
      })
    },
    [isStreaming, onInput, toNormalized],
  )

  if (error) {
    return (
      <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-1 bg-background/40 px-6 text-center">
        <div className="text-[12px] font-medium text-destructive">Emulator error</div>
        <div className="text-[11px] leading-relaxed text-muted-foreground/90">{error}</div>
      </div>
    )
  }

  if (!isStreaming || frameSrc === null || !currentDevice) {
    const headline =
      status.phase === "booting" || status.phase === "connecting"
        ? `Starting ${platformLabel}…`
        : isStreaming && !currentDevice
          ? `${platformLabel} streaming`
          : isStreaming
            ? `Waiting for first frame…`
            : `${platformLabel} not running`
    return (
      <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-1 bg-background/40 px-6 text-center">
        <div className="text-[12px] font-medium text-foreground/85">{headline}</div>
        <div className="text-[11px] leading-relaxed text-muted-foreground">
          {status.message ??
            `Pick a device above and hit Start to stream the ${platformLabel.toLowerCase()}.`}
        </div>
      </div>
    )
  }

  // Use the device's native pixel dimensions for the chassis aspect
  // ratio so the bezel hugs the simulator frame exactly. Fall back to
  // a generic 9:19.5 iPhone-ish ratio when the descriptor hasn't
  // hydrated yet.
  const aspectRatio =
    currentDevice?.width && currentDevice?.height
      ? `${currentDevice.width}/${currentDevice.height}`
      : "9/19.5"

  const isTablet = currentDevice?.kind === "tablet"
  const isIos = platform === "ios"

  return (
    <div
      className={cn(
        "relative flex min-h-0 flex-1 items-center justify-center bg-background/60 outline-none",
        "px-4 py-5",
        "focus-visible:ring-1 focus-visible:ring-primary/60 focus-visible:ring-offset-0",
      )}
      onKeyDown={onKeyDown}
      role="application"
      tabIndex={0}
    >
      <DeviceChassis
        aspectRatio={aspectRatio}
        isIos={isIos}
        isTablet={isTablet}
        orientation={orientation}
      >
        <div
          className={cn(
            "relative h-full w-full overflow-hidden bg-black",
            // Inner screen radius derives from the chassis radius minus
            // bezel thickness; tuned to match iPhone 15/17 Pro and a
            // generic Pixel-style Android chassis.
            isTablet
              ? "rounded-[14px]"
              : isIos
                ? "rounded-[34px]"
                : "rounded-[28px]",
          )}
          onPointerCancel={handlePointerUp}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          style={{ touchAction: "none" }}
        >
          <img
            ref={imgRef}
            alt={`${platformLabel} viewport`}
            className={cn(
              "block h-full w-full select-none object-cover transition-transform duration-150",
              orientation === "landscape" && "rotate-90",
            )}
            draggable={false}
            src={frameSrc}
          />
          {isIos && !isTablet ? <DynamicIsland /> : null}
          {isIos && !isTablet ? <HomeIndicator /> : null}
        </div>
      </DeviceChassis>
      <InputErrorToast message={inputError} onDismiss={onDismissInputError} />
    </div>
  )
}

/// iPhone-/Android-style physical bezel around the screen. Sizes itself
/// by aspect ratio so the chassis hugs the streamed image regardless of
/// device dimensions; max-h / max-w keep it inside the sidebar.
function DeviceChassis({
  aspectRatio,
  children,
  isIos,
  isTablet,
  orientation,
}: {
  aspectRatio: string
  children: React.ReactNode
  isIos: boolean
  isTablet: boolean
  orientation: EmulatorOrientation
}) {
  // Bezel thickness + outer corner radius mimic real-hardware
  // proportions at a UI-friendly size. iPad uses a shallower curve
  // since its physical corners are less aggressive than a Face ID
  // iPhone.
  const bezelClasses = isTablet
    ? "rounded-[20px] p-[8px]"
    : isIos
      ? "rounded-[42px] p-[8px]"
      : "rounded-[36px] p-[6px]"

  return (
    <div
      aria-hidden="true"
      className={cn(
        "relative flex items-stretch justify-stretch",
        "bg-gradient-to-b from-neutral-700/90 via-neutral-900 to-neutral-800/90",
        "shadow-[0_18px_45px_-18px_rgba(0,0,0,0.75)]",
        "ring-1 ring-white/10",
        "transition-transform duration-200 ease-out",
        bezelClasses,
        // In landscape we rotate the entire chassis so the bezel +
        // Dynamic Island land on the correct side of the screen,
        // matching what the user would see on a physical device held
        // horizontally.
        orientation === "landscape" && "rotate-90",
      )}
      style={{
        aspectRatio,
        maxHeight: "100%",
        maxWidth: "100%",
      }}
    >
      {children}
    </div>
  )
}

function DynamicIsland() {
  // Dimensions are relative to the screen so the island scales with
  // the chassis at any sidebar width. Real iPhone 15/17 Pro is ~126pt
  // wide and ~37pt tall — roughly 28% x 1.2% of device pixels.
  return (
    <div
      aria-hidden="true"
      className={cn(
        "pointer-events-none absolute left-1/2 -translate-x-1/2",
        "top-[0.95%] h-[3.5%] w-[28%]",
        "rounded-full bg-black",
        "shadow-[inset_0_0_0_1px_rgba(255,255,255,0.03)]",
      )}
    />
  )
}

function HomeIndicator() {
  // The thin horizontal pill at the bottom of every Face ID iPhone.
  // Sized in % so it tracks the chassis regardless of device.
  return (
    <div
      aria-hidden="true"
      className={cn(
        "pointer-events-none absolute left-1/2 -translate-x-1/2",
        "bottom-[0.5%] h-[0.35%] w-[32%] min-h-[3px]",
        "rounded-full bg-white/60",
      )}
    />
  )
}

function InputErrorToast({
  message,
  onDismiss,
}: {
  message: string | null
  onDismiss: () => void
}) {
  // Auto-dismiss after a few seconds so repeated input failures don't
  // pile up over the viewport. A manual dismiss button stays available
  // for cases where the user wants to clear it sooner.
  useEffect(() => {
    if (!message) return
    const handle = window.setTimeout(onDismiss, 6000)
    return () => window.clearTimeout(handle)
  }, [message, onDismiss])

  if (!message) return null

  return (
    <div
      aria-live="polite"
      className={cn(
        "pointer-events-auto absolute bottom-3 left-3 right-3 flex items-start gap-2",
        "rounded-md border border-red-500/40 bg-red-500/15 px-2.5 py-1.5 text-[11px] text-red-100",
        "shadow-md backdrop-blur-sm",
      )}
      role="status"
    >
      <span className="min-w-0 flex-1 break-words leading-snug">{message}</span>
      <button
        aria-label="Dismiss"
        className="shrink-0 rounded-sm p-0.5 text-red-100/80 hover:bg-red-500/20 hover:text-red-50"
        onClick={onDismiss}
        type="button"
      >
        <X className="h-3 w-3" />
      </button>
    </div>
  )
}
