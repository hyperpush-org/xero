import { useCallback, useEffect, useRef, useState } from "react"
import { invoke, isTauri } from "@tauri-apps/api/core"
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

interface BridgeAccount {
  githubLogin?: string | null
  avatarUrl?: string | null
}

interface BridgeStatusResponse {
  signedIn: boolean
  account?: BridgeAccount | null
}

interface BridgeAuthStatus {
  signedIn: boolean
  authorizationUrl?: string | null
  flowId?: string | null
  account?: BridgeAccount | null
}

const LOGIN_POLL_INTERVAL_MS = 1500
const LOGIN_TIMEOUT_MS = 5 * 60 * 1000

/** React hook backed by the shared desktop remote-bridge identity store. */
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

    setStatus((current) => (current === "authenticating" ? current : "loading"))
    try {
      const response = await invoke<BridgeStatusResponse>("bridge_status")
      if (!mountedRef.current) return
      const nextSession = response.signedIn ? sessionFromBridgeAccount(response.account) : null
      setSession(nextSession)
      setStatus(nextSession ? "ready" : "idle")
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
    const deadline = Date.now() + LOGIN_TIMEOUT_MS

    while (!signal.aborted && Date.now() < deadline) {
      try {
        const response = await invoke<BridgeAuthStatus>("bridge_poll_github_login", {
          request: { flowId },
        })

        if (!mountedRef.current || signal.aborted) return

        if (response.signedIn) {
          const nextSession = sessionFromBridgeAccount(response.account)
          setSession(nextSession)
          setStatus(nextSession ? "ready" : "idle")
          setError(null)
          return
        }
      } catch (caught) {
        if (!mountedRef.current || signal.aborted) return
        setError(toAuthError(caught))
        setStatus("error")
        return
      }

      await delay(LOGIN_POLL_INTERVAL_MS, signal)
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
      const started = await invoke<BridgeAuthStatus>("bridge_sign_in")
      if (started.signedIn) {
        const nextSession = sessionFromBridgeAccount(started.account)
        setSession(nextSession)
        setStatus(nextSession ? "ready" : "idle")
        return
      }

      const flowId = started.flowId?.trim()
      if (!flowId) {
        throw {
          code: "github_oauth_flow_missing",
          message: "GitHub login did not return a flow id.",
        } satisfies GitHubAuthError
      }

      pollAbortRef.current = new AbortController()
      void pollForCompletedLogin(flowId, pollAbortRef.current.signal)

      const authorizationUrl = started.authorizationUrl?.trim()
      if (authorizationUrl) {
        try {
          await openUrl(authorizationUrl)
        } catch (openErr) {
          console.warn("[github-auth] failed to open browser", openErr)
          setError({
            code: "github_open_browser_failed",
            message:
              "Could not open the system browser. Copy this URL into a browser to continue: " +
              authorizationUrl,
          })
          setStatus("authenticating")
        }
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
      pollAbortRef.current?.abort()
      await invoke<void>("bridge_sign_out")
      if (!mountedRef.current) return
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

function sessionFromBridgeAccount(
  account: BridgeAccount | null | undefined,
): GitHubSessionView | null {
  const login = account?.githubLogin?.trim()
  const avatarUrl = account?.avatarUrl?.trim() ?? ""
  if (!login && !avatarUrl) {
    return null
  }

  const safeLogin = login || "github"
  return {
    user: {
      id: 0,
      login: safeLogin,
      name: null,
      email: null,
      avatarUrl,
      htmlUrl: `https://github.com/${encodeURIComponent(safeLogin)}`,
    },
    scope: "",
    createdAt: "",
  }
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
    return value
  }
  if (value instanceof Error) {
    return {
      code: "github_auth_error",
      message: value.message,
    }
  }
  if (typeof value === "string" && value.trim()) {
    return {
      code: "github_auth_error",
      message: value,
    }
  }
  return {
    code: "github_auth_error",
    message: "GitHub authentication failed.",
  }
}

function isAuthError(value: unknown): value is GitHubAuthError {
  return (
    !!value &&
    typeof value === "object" &&
    typeof (value as GitHubAuthError).code === "string" &&
    typeof (value as GitHubAuthError).message === "string"
  )
}
