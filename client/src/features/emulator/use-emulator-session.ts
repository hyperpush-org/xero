import { useCallback, useEffect, useRef, useState } from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

export type EmulatorPlatform = "ios" | "android"

export type EmulatorPhase =
  | "idle"
  | "booting"
  | "connecting"
  | "streaming"
  | "stopping"
  | "stopped"
  | "error"

export interface EmulatorStatus {
  phase: EmulatorPhase
  platform?: string | null
  deviceId?: string | null
  message?: string | null
}

export interface EmulatorFrameInfo {
  seq: number
  width: number
  height: number
}

export interface DeviceDescriptor {
  id: string
  displayName: string
  kind: "phone" | "tablet"
  width: number
  height: number
  devicePixelRatio: number
}

export interface EmulatorStartResponse {
  platform: EmulatorPlatform
  deviceId: string
  width: number
  height: number
  devicePixelRatio: number
  frameUrl: string
}

export type EmulatorInputKind =
  | "touch_down"
  | "touch_move"
  | "touch_up"
  | "scroll"
  | "key"
  | "text"
  | "hw_button"

export interface EmulatorInputPayload {
  kind: EmulatorInputKind
  x?: number
  y?: number
  text?: string
  key?: string
  button?: string
}

export type EmulatorOrientation = "portrait" | "landscape"

export interface UseEmulatorSession {
  status: EmulatorStatus
  frame: EmulatorFrameInfo | null
  currentDevice: DeviceDescriptor | null
  devices: DeviceDescriptor[]
  isStarting: boolean
  isStopping: boolean
  /** Session-level error (boot failure, broken stream) — the canvas
   * is replaced with this. */
  error: string | null
  /** Transient error from a user-initiated action (tap, press key,
   * rotate). The canvas stays visible and this renders as a banner
   * that the UI can auto-dismiss. */
  inputError: string | null
  /** Clear the transient input error surface. */
  dismissInputError: () => void
  orientation: EmulatorOrientation
  refreshDevices: () => Promise<DeviceDescriptor[]>
  start: (deviceId: string) => Promise<EmulatorStartResponse | null>
  stop: () => Promise<void>
  sendInput: (input: EmulatorInputPayload) => Promise<void>
  pressKey: (key: string) => Promise<void>
  rotate: (orientation: EmulatorOrientation) => Promise<void>
}

const EMULATOR_FRAME_EVENT = "emulator:frame"
const EMULATOR_STATUS_EVENT = "emulator:status"

interface Options {
  platform: EmulatorPlatform
  /** Sidebar visibility — when false the hook releases any listeners. */
  active: boolean
}

function asTauri<T>(command: string, args?: Record<string, unknown>): Promise<T | null> {
  if (!isTauri()) return Promise.resolve(null)
  return invoke<T>(command, args).catch(() => null)
}

function errorMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message
    if (typeof message === "string" && message.length > 0) return message
  }
  if (typeof error === "string" && error.length > 0) return error
  return "Emulator command failed"
}

export function useEmulatorSession({ platform, active }: Options): UseEmulatorSession {
  const [status, setStatus] = useState<EmulatorStatus>({ phase: "idle" })
  const [frame, setFrame] = useState<EmulatorFrameInfo | null>(null)
  const [devices, setDevices] = useState<DeviceDescriptor[]>([])
  const [currentDevice, setCurrentDevice] = useState<DeviceDescriptor | null>(null)
  const [isStarting, setIsStarting] = useState(false)
  const [isStopping, setIsStopping] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [inputError, setInputError] = useState<string | null>(null)
  const devicesRef = useRef<DeviceDescriptor[]>([])
  devicesRef.current = devices
  const currentDeviceRef = useRef<DeviceDescriptor | null>(null)
  currentDeviceRef.current = currentDevice
  const platformTag = platform === "ios" ? "ios" : "android"

  // Wire backend status + frame events while the sidebar is visible.
  useEffect(() => {
    if (!active || !isTauri()) return
    let cancelled = false
    const unsubs: UnlistenFn[] = []

    void listen<{
      phase: EmulatorStatus["phase"]
      platform?: string | null
      deviceId?: string | null
      message?: string | null
    }>(EMULATOR_STATUS_EVENT, (event) => {
      if (cancelled) return
      const payload = event.payload
      // Ignore status updates for the other platform's session (mutex in
      // backend means there's only one, but the event is cross-platform).
      if (payload.platform && payload.platform !== platformTag) return
      setStatus({
        phase: payload.phase,
        platform: payload.platform ?? null,
        deviceId: payload.deviceId ?? null,
        message: payload.message ?? null,
      })
      if (payload.phase === "stopped" || payload.phase === "idle") {
        setFrame(null)
      }
      if (payload.message && payload.phase === "error") {
        setError(payload.message)
      }
    }).then((unsub) => {
      if (cancelled) {
        unsub()
      } else {
        unsubs.push(unsub)
      }
    })

    void listen<EmulatorFrameInfo>(EMULATOR_FRAME_EVENT, (event) => {
      if (cancelled) return
      setFrame(event.payload)
    }).then((unsub) => {
      if (cancelled) {
        unsub()
      } else {
        unsubs.push(unsub)
      }
    })

    // Ask the backend to re-emit the current status snapshot so we don't
    // have to wait for the next transition. The response also carries
    // the latest frame seq (if any) to defeat a startup race where
    // the first EMULATOR_FRAME_EVENT can fire before `listen()`
    // finishes registering.
    void invoke<{
      status: {
        phase: EmulatorStatus["phase"]
        platform?: string | null
        deviceId?: string | null
        message?: string | null
      }
      frame: EmulatorFrameInfo | null
    }>("emulator_subscribe_ready")
      .then((payload) => {
        if (cancelled) return
        const statusPayload = payload.status
        if (!statusPayload.platform || statusPayload.platform === platformTag) {
          setStatus({
            phase: statusPayload.phase,
            platform: statusPayload.platform ?? null,
            deviceId: statusPayload.deviceId ?? null,
            message: statusPayload.message ?? null,
          })
          if (payload.frame) {
            setFrame((current) => current ?? payload.frame)
          }
        }
      })
      .catch(() => {
        /* subscribe is best-effort */
      })

    return () => {
      cancelled = true
      unsubs.forEach((unsub) => unsub())
    }
  }, [active, platformTag])

  const refreshDevices = useCallback(async (): Promise<DeviceDescriptor[]> => {
    setError(null)
    const list = await asTauri<DeviceDescriptor[]>("emulator_list_devices", { request: { platform: platformTag } })
    if (!list) {
      setDevices([])
      return []
    }
    setDevices(list)
    return list
  }, [platformTag])

  useEffect(() => {
    if (!active) return
    void refreshDevices()
  }, [active, refreshDevices])

  const start = useCallback(
    async (deviceId: string): Promise<EmulatorStartResponse | null> => {
      if (!isTauri()) return null
      setError(null)
      setIsStarting(true)
      try {
        const response = await invoke<EmulatorStartResponse>("emulator_start", {
          request: { platform: platformTag, deviceId },
        })
        const descriptor = devicesRef.current.find((d) => d.id === deviceId) ?? {
          id: deviceId,
          displayName: deviceId,
          kind: "phone" as const,
          width: response.width,
          height: response.height,
          devicePixelRatio: response.devicePixelRatio,
        }
        setCurrentDevice(descriptor)
        return response
      } catch (err) {
        setError(errorMessage(err))
        return null
      } finally {
        setIsStarting(false)
      }
    },
    [platformTag],
  )

  const stop = useCallback(async (): Promise<void> => {
    if (!isTauri()) return
    setIsStopping(true)
    try {
      await invoke("emulator_stop")
    } catch (err) {
      setError(errorMessage(err))
    } finally {
      setIsStopping(false)
      setCurrentDevice(null)
      setFrame(null)
    }
  }, [])

  const sendInput = useCallback(async (input: EmulatorInputPayload): Promise<void> => {
    if (!isTauri()) return
    try {
      await invoke("emulator_input", { request: input })
    } catch (err) {
      // Input commands fire at high frequency (touch_move runs at
      // ~60Hz during a drag). Route failures to the transient surface
      // so they don't replace the video canvas — streamed frames are
      // still arriving and the user can keep interacting.
      setInputError(errorMessage(err))
    }
  }, [])

  const pressKey = useCallback(async (key: string): Promise<void> => {
    if (!isTauri()) return
    try {
      await invoke("emulator_press_key", { request: { key } })
    } catch (err) {
      setInputError(errorMessage(err))
    }
  }, [])

  const [orientation, setOrientation] = useState<EmulatorOrientation>("portrait")

  const rotate = useCallback(
    async (next: EmulatorOrientation): Promise<void> => {
      if (!isTauri()) {
        setOrientation(next)
        return
      }
      try {
        await invoke("emulator_rotate", { request: { orientation: next } })
        setOrientation(next)
      } catch (err) {
        setInputError(errorMessage(err))
      }
    },
    [],
  )

  const dismissInputError = useCallback(() => {
    setInputError(null)
  }, [])

  // Any session-level transition clears the input-error banner — a
  // broken rotate isn't worth surfacing after the user starts a fresh
  // device.
  useEffect(() => {
    if (status.phase === "idle" || status.phase === "stopped") {
      setInputError(null)
    }
  }, [status.phase])

  // Whenever phase transitions into streaming without a frame already
  // cached, ask the backend for the latest frame info directly. This
  // covers the case where listen() finishes registering after the first
  // frame was emitted — subscribe_ready returns the current FrameBus
  // head so the <img> can start rendering right away.
  useEffect(() => {
    if (status.phase !== "streaming" || frame) return
    if (!isTauri()) return
    let cancelled = false
    void invoke<{ frame: EmulatorFrameInfo | null }>("emulator_subscribe_ready")
      .then((payload) => {
        if (cancelled) return
        if (payload.frame) {
          setFrame((current) => current ?? payload.frame)
        }
      })
      .catch(() => {
        /* best-effort re-seed */
      })
    return () => {
      cancelled = true
    }
  }, [status.phase, frame])

  return {
    status,
    frame,
    currentDevice,
    devices,
    isStarting,
    isStopping,
    error,
    inputError,
    dismissInputError,
    orientation,
    refreshDevices,
    start,
    stop,
    sendInput,
    pressKey,
    rotate,
  }
}
