"use client"

import { useCallback, useRef, useState } from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"

// Types matching Rust backend structs.

export interface SourceLocation {
  file: string
  line: number
  column: number
}

export interface ElementBounds {
  x: number
  y: number
  w: number
  h: number
}

export interface ElementInfo {
  componentName: string | null
  nativeType: string | null
  bounds: ElementBounds
  props: Record<string, unknown>
  source: SourceLocation | null
}

export interface MetroStatus {
  connected: boolean
  port: number
  pages: Array<{ id: string; title: string; vm: string; description: string }>
}

export interface UseInspector {
  /** Whether inspect mode is active. */
  inspectMode: boolean
  /** Toggle inspect mode on/off. */
  toggleInspect: () => void
  /** Whether the Metro inspector is connected. */
  metroConnected: boolean
  /** Status of the Metro connection. */
  metroStatus: MetroStatus | null
  /** The element currently hovered (null if none). */
  hoveredElement: ElementInfo | null
  /** Error from the inspector. */
  inspectError: string | null
  /** Connect to Metro inspector (auto-discover or explicit port). */
  connect: (port?: number) => Promise<void>
  /** Disconnect from Metro inspector. */
  disconnect: () => Promise<void>
  /** Query element at a point (device pixels). */
  elementAt: (x: number, y: number) => Promise<ElementInfo | null>
  /** Get the full component tree. */
  componentTree: () => Promise<unknown>
}

/**
 * Hook for Metro inspector integration. Provides element-at-point
 * inspection, source mapping, and highlight overlays for React Native
 * and Expo apps.
 */
export function useInspector(): UseInspector {
  const [inspectMode, setInspectMode] = useState(false)
  const [metroStatus, setMetroStatus] = useState<MetroStatus | null>(null)
  const [hoveredElement, setHoveredElement] = useState<ElementInfo | null>(null)
  const [inspectError, setInspectError] = useState<string | null>(null)

  // Debounce timer for element-at-point queries.
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const toggleInspect = useCallback(() => {
    setInspectMode((prev) => {
      if (prev) {
        // Turning off — clear hovered element.
        setHoveredElement(null)
      }
      return !prev
    })
  }, [])

  const connect = useCallback(async (port?: number) => {
    if (!isTauri()) return
    setInspectError(null)
    try {
      const status = await invoke<MetroStatus>("emulator_inspector_connect", {
        request: { port: port ?? null },
      })
      setMetroStatus(status)
    } catch (err) {
      setInspectError(errorMessage(err))
      setMetroStatus(null)
    }
  }, [])

  const disconnect = useCallback(async () => {
    if (!isTauri()) return
    try {
      await invoke("emulator_inspector_disconnect")
    } finally {
      setMetroStatus(null)
      setHoveredElement(null)
    }
  }, [])

  const elementAt = useCallback(async (x: number, y: number): Promise<ElementInfo | null> => {
    if (!isTauri()) return null
    try {
      const info = await invoke<ElementInfo>("emulator_inspector_element_at", {
        request: { x, y },
      })
      setHoveredElement(info)
      setInspectError(null)
      return info
    } catch (err) {
      // Transient — don't flood UI with errors on hover.
      setHoveredElement(null)
      return null
    }
  }, [])

  const elementAtDebounced = useCallback(
    (x: number, y: number) => {
      if (debounceRef.current) clearTimeout(debounceRef.current)
      return new Promise<ElementInfo | null>((resolve) => {
        debounceRef.current = setTimeout(async () => {
          resolve(await elementAt(x, y))
        }, 80)
      })
    },
    [elementAt],
  )

  const componentTree = useCallback(async () => {
    if (!isTauri()) return null
    try {
      return await invoke("emulator_inspector_component_tree")
    } catch (err) {
      setInspectError(errorMessage(err))
      return null
    }
  }, [])

  return {
    inspectMode,
    toggleInspect,
    metroConnected: metroStatus?.connected ?? false,
    metroStatus,
    hoveredElement,
    inspectError,
    connect,
    disconnect,
    elementAt: elementAtDebounced,
    componentTree,
  }
}

function errorMessage(err: unknown): string {
  if (err && typeof err === "object" && "message" in err) {
    const msg = (err as { message?: unknown }).message
    if (typeof msg === "string" && msg.length > 0) return msg
  }
  if (typeof err === "string" && err.length > 0) return err
  return "Inspector error"
}
