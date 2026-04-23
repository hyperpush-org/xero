"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  Apple,
  Loader2,
  Play,
  RotateCcw,
  Square,
  Smartphone,
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
        isStreaming={isStreaming}
        onInput={(payload) => void session.sendInput(payload)}
        onKeyDown={handleKeyDown}
        orientation={session.orientation}
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
  isStreaming: boolean
  onInput: (input: { kind: EmulatorInputKind; x?: number; y?: number }) => void
  onKeyDown: (event: React.KeyboardEvent<HTMLDivElement>) => void
  orientation: EmulatorOrientation
  platformLabel: string
  status: ReturnType<typeof useEmulatorSession>["status"]
}

function EmulatorViewport({
  currentDevice,
  error,
  frameSeq,
  isStreaming,
  onInput,
  onKeyDown,
  orientation,
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
    return (
      <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-1 bg-background/40 px-6 text-center">
        <div className="text-[12px] font-medium text-foreground/85">
          {status.phase === "booting" || status.phase === "connecting"
            ? `Starting ${platformLabel}…`
            : `${platformLabel} not running`}
        </div>
        <div className="text-[11px] leading-relaxed text-muted-foreground">
          {status.message ??
            `Pick a device above and hit Start to stream the ${platformLabel.toLowerCase()}.`}
        </div>
      </div>
    )
  }

  return (
    <div
      className={cn(
        "relative flex min-h-0 flex-1 items-center justify-center bg-black outline-none",
        "focus-visible:ring-1 focus-visible:ring-primary/60 focus-visible:ring-offset-0",
      )}
      onKeyDown={onKeyDown}
      onPointerCancel={handlePointerUp}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      role="application"
      style={{ touchAction: "none" }}
      tabIndex={0}
    >
      <img
        ref={imgRef}
        alt={`${platformLabel} viewport`}
        className={cn(
          "max-h-full max-w-full select-none transition-transform duration-150",
          orientation === "landscape" && "rotate-90",
        )}
        draggable={false}
        src={frameSrc}
      />
    </div>
  )
}
