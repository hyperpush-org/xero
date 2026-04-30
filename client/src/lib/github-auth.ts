import { useCallback, useEffect, useRef, useState } from "react"
import { isTauri } from "@tauri-apps/api/core"
import { openUrl } from "@tauri-apps/plugin-opener"

export interface GitHubUser {
  id: number
  login: string
  name: string | null
  email: string | null
  avatarUrl: string
  htmlUrl: string
}

export interface GitHubSessionView {
  user: GitHubUser
  scope: string
  createdAt: string
}

export interface StartedGitHubLogin {
  authorizationUrl: string
  redirectUri: string
  flowId: string
}

export interface GitHubAuthError {
  code: string
  message: string
}

export type GitHubAuthStatus = "idle" | "loading" | "authenticating" | "ready" | "error"

export interface UseGitHubAuthResult {
  session: GitHubSessionView | null
  status: GitHubAuthStatus
  error: GitHubAuthError | null
  login: () => Promise<void>
  logout: () => Promise<void>
  refresh: () => Promise<void>
}

const GITHUB_SESSION_ID_STORAGE_KEY = "xero.github.sessionId"
const SERVER_BASE_URL =
  (import.meta.env.VITE_XERO_SERVER_URL as string | undefined)?.replace(/\/+$/, "") ??
  "http://127.0.0.1:4000"

type FlowSessionResponse =
  | { status: "pending" }
  | { status: "ready"; sessionId: string; session: GitHubSessionView }

/** React hook that asks the server to own GitHub OAuth for the current user. */
export function useGitHubAuth(): UseGitHubAuthResult {
  const [session, setSession] = useState<GitHubSessionView | null>(null)
  const [status, setStatus] = useState<GitHubAuthStatus>("idle")
  const [error, setError] = useState<GitHubAuthError | null>(null)
  const mountedRef = useRef(true)
  const pollAbortRef = useRef<AbortController | null>(null)

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      pollAbortRef.current?.abort()
    }
  }, [])

  const refresh = useCallback(async () => {
    if (!isTauri()) {
      return
    }
    const sessionId = window.localStorage.getItem(GITHUB_SESSION_ID_STORAGE_KEY)
    if (!sessionId) {
      setSession(null)
      setStatus("idle")
      setError(null)
      return
    }
    try {
      const response = await requestJson<{ session: GitHubSessionView | null }>("/api/github/session", {
        headers: sessionHeaders(sessionId),
      })
      if (!mountedRef.current) return
      if (response.session) {
        setSession(response.session)
        setStatus("ready")
      } else {
        window.localStorage.removeItem(GITHUB_SESSION_ID_STORAGE_KEY)
        setSession(null)
        setStatus("idle")
      }
      setError(null)
    } catch (caught) {
      if (!mountedRef.current) return
      setError(toAuthError(caught))
      setStatus("error")
    }
  }, [])

  useEffect(() => {
    if (!isTauri()) return
    void refresh()
  }, [refresh])

  const pollForCompletedLogin = useCallback(async (flowId: string, signal: AbortSignal) => {
    const deadline = Date.now() + 5 * 60 * 1000

    while (!signal.aborted && Date.now() < deadline) {
      try {
        const response = await requestJson<FlowSessionResponse>(
          `/api/github/session?flowId=${encodeURIComponent(flowId)}`,
          { signal },
        )

        if (!mountedRef.current || signal.aborted) return

        if (response.status === "ready") {
          window.localStorage.setItem(GITHUB_SESSION_ID_STORAGE_KEY, response.sessionId)
          setSession(response.session)
          setStatus("ready")
          setError(null)
          return
        }
      } catch (caught) {
        if (!mountedRef.current || signal.aborted) return
        setError(toAuthError(caught))
        setStatus("error")
        return
      }

      await delay(1500, signal)
    }

    if (!mountedRef.current || signal.aborted) return
    setError({
      code: "github_oauth_timeout",
      message: "GitHub sign in timed out before the server received the callback.",
    })
    setStatus("error")
  }, [])

  const login = useCallback(async () => {
    if (!isTauri()) {
      setError({
        code: "github_oauth_unavailable",
        message: "GitHub login is only available in the desktop app.",
      })
      setStatus("error")
      return
    }

    setStatus("authenticating")
    setError(null)
    try {
      pollAbortRef.current?.abort()
      const started = await requestJson<StartedGitHubLogin>("/api/github/login", { method: "POST" })
      pollAbortRef.current = new AbortController()
      void pollForCompletedLogin(started.flowId, pollAbortRef.current.signal)
      try {
        await openUrl(started.authorizationUrl)
      } catch (openErr) {
        console.warn("[github-auth] failed to open browser", openErr)
        setError({
          code: "github_open_browser_failed",
          message:
            "Could not open the system browser. Copy this URL into a browser to continue: " +
            started.authorizationUrl,
        })
        setStatus("authenticating")
      }
    } catch (caught) {
      if (!mountedRef.current) return
      setError(toAuthError(caught))
      setStatus("error")
    }
  }, [pollForCompletedLogin])

  const logout = useCallback(async () => {
    if (!isTauri()) {
      setSession(null)
      setStatus("idle")
      return
    }
    try {
      const sessionId = window.localStorage.getItem(GITHUB_SESSION_ID_STORAGE_KEY)
      if (sessionId) {
        await requestJson<void>("/api/github/session", {
          method: "DELETE",
          headers: sessionHeaders(sessionId),
        })
      }
      if (!mountedRef.current) return
      window.localStorage.removeItem(GITHUB_SESSION_ID_STORAGE_KEY)
      setSession(null)
      setStatus("idle")
      setError(null)
    } catch (caught) {
      if (!mountedRef.current) return
      setError(toAuthError(caught))
      setStatus("error")
    }
  }, [])

  return { session, status, error, login, logout, refresh }
}

async function requestJson<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers)
  if (init.body && !headers.has("content-type")) {
    headers.set("content-type", "application/json")
  }

  const response = await fetch(`${SERVER_BASE_URL}${path}`, {
    ...init,
    headers,
  })

  if (response.status === 204) {
    return undefined as T
  }

  const body = (await response.json().catch(() => null)) as unknown

  if (!response.ok) {
    if (
      body &&
      typeof body === "object" &&
      "error" in body &&
      isAuthError((body as { error: unknown }).error)
    ) {
      throw (body as { error: GitHubAuthError }).error
    }

    throw {
      code: "github_server_error",
      message: `Xero server returned HTTP ${response.status} for GitHub auth.`,
    } satisfies GitHubAuthError
  }

  return body as T
}

function sessionHeaders(sessionId: string): Record<string, string> {
  return { "x-xero-github-session-id": sessionId }
}

function delay(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    const timer = window.setTimeout(resolve, ms)
    signal.addEventListener(
      "abort",
      () => {
        window.clearTimeout(timer)
        resolve()
      },
      { once: true },
    )
  })
}

function toAuthError(value: unknown): GitHubAuthError {
  if (isAuthError(value)) {
    return value as GitHubAuthError
  }
  return {
    code: "github_unknown_error",
    message: typeof value === "string" ? value : "An unexpected error occurred.",
  }
}

function isAuthError(value: unknown): value is GitHubAuthError {
  return (
    value !== null &&
    typeof value === "object" &&
    "code" in value &&
    "message" in value &&
    typeof (value as { code: unknown }).code === "string" &&
    typeof (value as { message: unknown }).message === "string"
  )
}
