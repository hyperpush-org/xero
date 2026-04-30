import { useCallback, useEffect, useRef, useState } from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"

export interface DetectedBrowser {
  id: string
  label: string
  available: boolean
}

export interface CookieImportResult {
  source: string
  imported: number
  skipped: number
  domains: number
}

export type CookieImportStatus =
  | { kind: "idle" }
  | { kind: "running"; source: string }
  | { kind: "success"; source: string; result: CookieImportResult }
  | { kind: "error"; source: string; message: string }

export interface UseCookieImport {
  browsers: DetectedBrowser[]
  status: CookieImportStatus
  refresh: () => Promise<DetectedBrowser[]>
  importFrom: (browser: DetectedBrowser) => Promise<void>
  reset: () => void
}

export function useCookieImport(options?: { autoLoad?: boolean }): UseCookieImport {
  const [browsers, setBrowsers] = useState<DetectedBrowser[]>([])
  const [status, setStatus] = useState<CookieImportStatus>({ kind: "idle" })
  const loadedRef = useRef(false)

  const refresh = useCallback(async () => {
    if (!isTauri()) {
      setBrowsers([])
      return []
    }
    try {
      const list = (await invoke<DetectedBrowser[]>("browser_list_cookie_sources")) ?? []
      setBrowsers(list)
      return list
    } catch {
      setBrowsers([])
      return []
    }
  }, [])

  useEffect(() => {
    if (!options?.autoLoad) return
    if (loadedRef.current) return
    loadedRef.current = true
    void refresh()
  }, [options?.autoLoad, refresh])

  const importFrom = useCallback(async (browser: DetectedBrowser) => {
    if (!isTauri()) return
    setStatus({ kind: "running", source: browser.id })
    try {
      const result = await invoke<CookieImportResult>("browser_import_cookies", {
        source: browser.id,
      })
      setStatus({ kind: "success", source: browser.id, result })
    } catch (error) {
      const message =
        typeof error === "object" && error && "message" in error
          ? String((error as { message?: unknown }).message ?? "")
          : String(error)
      setStatus({
        kind: "error",
        source: browser.id,
        message: message || `Could not import from ${browser.label}.`,
      })
    }
  }, [])

  const reset = useCallback(() => setStatus({ kind: "idle" }), [])

  return { browsers, status, refresh, importFrom, reset }
}
