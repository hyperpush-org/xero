const GITHUB_SESSION_ID_STORAGE_KEY = "xero.github.sessionId"
const SERVER_BASE_URL =
  (import.meta.env.VITE_XERO_SERVER_URL as string | undefined)?.replace(/\/+$/, "") ??
  "http://127.0.0.1:4000"

export interface GameLeaderboardEntryDto {
  githubUserId: number
  login: string
  name: string | null
  avatarUrl: string | null
  score: number
  you: boolean
}

export interface GameStatDto {
  gameId: string
  personalBest: number
  runs: number
  timePlayedMs: number
  lastPlayedAt: string | null
  leaderboard: GameLeaderboardEntryDto[]
}

export interface GameStatsSnapshotDto {
  stats: GameStatDto[]
}

export interface GameRunInput {
  gameId: string
  score: number
  timePlayedMs: number
}

export async function loadGameStats(): Promise<GameStatsSnapshotDto | null> {
  const sessionId = githubSessionId()
  if (!sessionId) return null

  return requestJson<GameStatsSnapshotDto>("/api/games/stats", {
    headers: sessionHeaders(sessionId),
  })
}

export async function recordGameRun(run: GameRunInput): Promise<GameStatsSnapshotDto | null> {
  const sessionId = githubSessionId()
  if (!sessionId) return null

  return requestJson<GameStatsSnapshotDto>("/api/games/runs", {
    method: "POST",
    headers: sessionHeaders(sessionId),
    body: JSON.stringify(run),
  })
}

function githubSessionId(): string | null {
  if (typeof window === "undefined") return null
  return window.localStorage.getItem(GITHUB_SESSION_ID_STORAGE_KEY)
}

async function requestJson<T>(path: string, init: RequestInit): Promise<T> {
  const headers = new Headers(init.headers)
  if (init.body && !headers.has("content-type")) {
    headers.set("content-type", "application/json")
  }

  const response = await fetch(`${SERVER_BASE_URL}${path}`, {
    ...init,
    headers,
  })

  const body = (await response.json().catch(() => null)) as unknown

  if (!response.ok) {
    throw body
  }

  return body as T
}

function sessionHeaders(sessionId: string): Record<string, string> {
  return { "x-xero-github-session-id": sessionId }
}
