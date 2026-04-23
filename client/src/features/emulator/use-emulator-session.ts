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
  error: string | null
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
    // have to wait for the next transition.
    void invoke<{
      phase: EmulatorStatus["phase"]
      platform?: string | null
      deviceId?: string | null
      message?: string | null
    }>("emulator_subscribe_ready")
      .then((payload) => {
        if (cancelled) return
        if (!payload.platform || payload.platform === platformTag) {
          setStatus({
            phase: payload.phase,
            platform: payload.platform ?? null,
            deviceId: payload.deviceId ?? null,
            message: payload.message ?? null,
          })
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
      // Input commands fire at high frequency; swallow individual failures to
      // avoid flooding the error surface, but remember the last one.
      setError(errorMessage(err))
    }
  }, [])

  const pressKey = useCallback(async (key: string): Promise<void> => {
    if (!isTauri()) return
    try {
      await invoke("emulator_press_key", { request: { key } })
    } catch (err) {
      setError(errorMessage(err))
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
        setError(errorMessage(err))
      }
    },
    [],
  )

  return {
    status,
    frame,
    currentDevice,
    devices,
    isStarting,
    isStopping,
    error,
    orientation,
    refreshDevices,
    start,
    stop,
    sendInput,
    pressKey,
    rotate,
  }
}
