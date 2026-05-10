import { useCallback, useEffect, useMemo, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { relaunch } from '@tauri-apps/plugin-process'
import { check, type Update } from '@tauri-apps/plugin-updater'

type ForcedUpdateStatus =
  | 'checking'
  | 'downloading'
  | 'installing'
  | 'error'
  | 'up-to-date'
  | 'skipped'

interface ForcedUpdateProgress {
  downloaded: number
  total: number | null
  percent: number
}

interface ForcedUpdateState {
  status: ForcedUpdateStatus
  version: string | null
  progress: ForcedUpdateProgress
  error: string | null
}

const EMPTY_PROGRESS: ForcedUpdateProgress = {
  downloaded: 0,
  total: null,
  percent: 0,
}

function shouldSkipForcedUpdateCheck(): boolean {
  if (import.meta.env.DEV || import.meta.env.MODE === 'test') {
    return true
  }

  try {
    return !isTauri()
  } catch {
    return true
  }
}

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message
  }
  if (typeof error === 'string' && error.trim()) {
    return error
  }
  return 'Xero could not install the required update.'
}

async function checkWithTimeout(): Promise<Update | null> {
  let timeout: number | null = null

  try {
    return await Promise.race([
      check(),
      new Promise<never>((_, reject) => {
        timeout = window.setTimeout(
          () => reject(new Error('The update check timed out.')),
          15_000,
        )
      }),
    ])
  } finally {
    if (timeout !== null) {
      window.clearTimeout(timeout)
    }
  }
}

export function useForcedAppUpdate() {
  const skipped = useMemo(shouldSkipForcedUpdateCheck, [])
  const [state, setState] = useState<ForcedUpdateState>(() => ({
    status: skipped ? 'skipped' : 'checking',
    version: null,
    progress: EMPTY_PROGRESS,
    error: null,
  }))

  const runUpdateCheck = useCallback(async () => {
    if (skipped) {
      setState({
        status: 'skipped',
        version: null,
        progress: EMPTY_PROGRESS,
        error: null,
      })
      return
    }

    setState({
      status: 'checking',
      version: null,
      progress: EMPTY_PROGRESS,
      error: null,
    })

    let update: Update | null = null

    try {
      update = await checkWithTimeout()
    } catch (error) {
      console.warn('[updates] update check failed; continuing startup', error)
      setState({
        status: 'up-to-date',
        version: null,
        progress: EMPTY_PROGRESS,
        error: null,
      })
      return
    }

    if (!update) {
      setState({
        status: 'up-to-date',
        version: null,
        progress: EMPTY_PROGRESS,
        error: null,
      })
      return
    }

    setState({
      status: 'downloading',
      version: update.version,
      progress: EMPTY_PROGRESS,
      error: null,
    })

    let downloaded = 0
    let total: number | null = null

    try {
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          downloaded = 0
          total = event.data.contentLength ?? null
          setState((current) => ({
            ...current,
            status: 'downloading',
            progress: {
              downloaded,
              total,
              percent: 0,
            },
          }))
          return
        }

        if (event.event === 'Progress') {
          downloaded += event.data.chunkLength
          const percent =
            total && total > 0
              ? Math.min(99, Math.round((downloaded / total) * 100))
              : 0

          setState((current) => ({
            ...current,
            status: 'downloading',
            progress: {
              downloaded,
              total,
              percent,
            },
          }))
          return
        }

        if (event.event === 'Finished') {
          setState((current) => ({
            ...current,
            status: 'installing',
            progress: {
              downloaded,
              total,
              percent: 100,
            },
          }))
        }
      })

      setState((current) => ({
        ...current,
        status: 'installing',
        progress: {
          downloaded,
          total,
          percent: 100,
        },
      }))

      window.setTimeout(() => {
        void relaunch().catch((error) => {
          setState((current) => ({
            ...current,
            status: 'error',
            error: errorMessage(error),
          }))
        })
      }, 500)
    } catch (error) {
      setState((current) => ({
        ...current,
        status: 'error',
        error: errorMessage(error),
      }))
    }
  }, [skipped])

  useEffect(() => {
    void runUpdateCheck()
  }, [runUpdateCheck])

  return {
    ...state,
    canContinue: state.status === 'skipped' || state.status === 'up-to-date',
    retry: runUpdateCheck,
  }
}
