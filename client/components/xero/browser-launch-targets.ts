export interface BrowserLaunchTarget {
  id: string
  label: string
  projectId?: string | null
  url: string
  source?: string | null
  detectedAt: number
}

export interface BrowserLaunchTargetInput {
  label: string
  projectId?: string | null
  url: string
  source?: string | null
  detectedAt?: number
}

export interface BrowserServerLabelStartTarget {
  name: string
  command: string
  browserSupported?: boolean | null
}

export interface BrowserRunningServerLabelInput {
  cwd?: string | null
  label?: string | null
  processName?: string | null
  url: string
}

const ANSI_ESCAPE_PATTERN =
  /[\u001b\u009b][[\]()#;?]*(?:[0-9]{1,4}(?:;[0-9]{0,4})*)?[0-9A-ORZcf-nqry=><]/g
const LOCAL_DEV_URL_PATTERN =
  /\bhttps?:\/\/(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\])(?::\d{2,5})?(?:\/[^\s"'<>]*)?/gi

export function isBrowserSupportedDevServerUrl(value: string): boolean {
  try {
    const url = new URL(value)
    if (url.protocol !== "http:" && url.protocol !== "https:") return false
    const host = url.hostname.toLowerCase()
    return (
      host === "localhost" ||
      host === "127.0.0.1" ||
      host === "0.0.0.0" ||
      host === "[::1]" ||
      host === "::1"
    )
  } catch {
    return false
  }
}

export function normalizeBrowserLaunchUrl(value: string): string | null {
  try {
    const url = new URL(value.trim())
    if (!isBrowserSupportedDevServerUrl(url.toString())) return null
    return normalizeLoopbackBrowserUrl(url.toString())
  } catch {
    return null
  }
}

export function normalizeLoopbackBrowserUrl(value: string): string {
  const trimmed = value.trim()
  try {
    const url = new URL(trimmed)
    if (url.protocol !== "http:" && url.protocol !== "https:") return value
    const host = url.hostname.toLowerCase()
    if (host === "localhost" || host === "0.0.0.0") {
      url.hostname = "127.0.0.1"
      return url.toString()
    }
    return trimmed
  } catch {
    return value
  }
}

export function browserLaunchTargetId(url: string): string {
  const normalized = normalizeBrowserLaunchUrl(url) ?? url.trim()
  return `browser-app:${normalized.toLowerCase()}`
}

function browserLaunchOriginKey(value: string): string | null {
  const normalized = normalizeBrowserLaunchUrl(value)
  if (!normalized) return null
  try {
    const url = new URL(normalized)
    return url.origin.toLowerCase()
  } catch {
    return null
  }
}

export function browserLaunchTargetMatchesUrl(target: BrowserLaunchTarget, url: string): boolean {
  const targetOrigin = browserLaunchOriginKey(target.url)
  const unavailableOrigin = browserLaunchOriginKey(url)
  return targetOrigin !== null && targetOrigin === unavailableOrigin
}

function browserLaunchTargetHost(url: string): string {
  try {
    const parsed = new URL(url)
    return parsed.port ? `${parsed.hostname}:${parsed.port}` : parsed.hostname
  } catch {
    return "local app"
  }
}

export function browserLaunchTargetLabel(url: string, source?: string | null): string {
  const host = browserLaunchTargetHost(url)
  const sourceLabel = source?.trim()
  return sourceLabel ? `${sourceLabel} · ${host}` : host
}

export function makeBrowserLaunchTarget(input: BrowserLaunchTargetInput): BrowserLaunchTarget | null {
  const url = normalizeBrowserLaunchUrl(input.url)
  if (!url) return null
  const source = input.source?.trim() || null
  return {
    id: browserLaunchTargetId(url),
    label: input.label.trim() || browserLaunchTargetLabel(url, source),
    projectId: input.projectId ?? null,
    url,
    source,
    detectedAt: input.detectedAt ?? Date.now(),
  }
}

export function extractBrowserSupportedDevServerUrls(data: string): string[] {
  const clean = data.replace(ANSI_ESCAPE_PATTERN, "")
  const urls = new Set<string>()
  for (const match of clean.matchAll(LOCAL_DEV_URL_PATTERN)) {
    const normalized = normalizeBrowserLaunchUrl(match[0])
    if (normalized) urls.add(normalized)
  }
  return Array.from(urls)
}

function normalizePathForBrowserLabel(value: string): string {
  return value
    .replace(/\\/g, "/")
    .replace(/\/+/g, "/")
    .replace(/\/$/, "")
}

function unquoteShellToken(value: string): string {
  const trimmed = value.trim()
  if (
    (trimmed.startsWith("\"") && trimmed.endsWith("\"")) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'"))
  ) {
    return trimmed.slice(1, -1)
  }
  return trimmed
}

function joinBrowserLabelPath(root: string | null, value: string): string | null {
  const path = unquoteShellToken(value)
  if (!path || path === ".") return root ? normalizePathForBrowserLabel(root) : null
  if (path.startsWith("/") || /^[a-zA-Z]:[\\/]/.test(path)) {
    return normalizePathForBrowserLabel(path)
  }
  if (!root) return normalizePathForBrowserLabel(path)
  return normalizePathForBrowserLabel(`${root}/${path}`)
}

function commandWorkingDirectory(command: string, projectRootPath?: string | null): string | null {
  const root = projectRootPath ? normalizePathForBrowserLabel(projectRootPath) : null
  const cdMatch = command.match(/(?:^|[;&|]\s*)cd\s+("[^"]+"|'[^']+'|[^\s;&|]+)/)
  if (cdMatch?.[1]) return joinBrowserLabelPath(root, cdMatch[1])

  const packageDirMatch = command.match(
    /\b(?:pnpm|npm|yarn)\s+(?:--dir|-C|--prefix|--cwd)\s+("[^"]+"|'[^']+'|[^\s;&|]+)/,
  )
  if (packageDirMatch?.[1]) return joinBrowserLabelPath(root, packageDirMatch[1])

  const manifestMatch = command.match(/\b--manifest-path\s+("[^"]+"|'[^']+'|[^\s;&|]+)/)
  if (manifestMatch?.[1]) {
    const manifestPath = unquoteShellToken(manifestMatch[1]).replace(/\\/g, "/")
    const dir = manifestPath.split("/").slice(0, -1).join("/") || "."
    return joinBrowserLabelPath(root, dir)
  }

  return root
}

function pathContainsOrEquals(parent: string, child: string): boolean {
  return child === parent || child.startsWith(`${parent}/`)
}

function commandPorts(command: string): number[] {
  const ports = new Set<number>()
  const patterns = [
    /\b[A-Z0-9_]*PORT\s*=\s*(\d{2,5})\b/g,
    /\b(?:--port|-p)\s*=?\s*(\d{2,5})\b/g,
    /\b(?:localhost|127\.0\.0\.1|0\.0\.0\.0):(\d{2,5})\b/g,
  ]
  for (const pattern of patterns) {
    for (const match of command.matchAll(pattern)) {
      const port = Number(match[1])
      if (Number.isInteger(port) && port > 0 && port <= 65535) ports.add(port)
    }
  }

  const lower = command.toLowerCase()
  if (lower.includes("phx.server")) ports.add(4000)
  if (lower.includes("storybook")) ports.add(6006)
  if (lower.includes("astro")) ports.add(4321)
  if (lower.includes("vite")) ports.add(5173)
  if (/\bnext\s+dev\b/.test(lower) || /\bnuxt\s+dev\b/.test(lower)) ports.add(3000)
  if (lower.includes("expo") || lower.includes("metro")) ports.add(8081)
  return Array.from(ports)
}

function browserServerPort(url: string): number | null {
  try {
    const parsed = new URL(url)
    const port = Number(parsed.port || (parsed.protocol === "https:" ? 443 : 80))
    return Number.isInteger(port) ? port : null
  } catch {
    return null
  }
}

function scoreStartTargetForServer(
  target: BrowserServerLabelStartTarget,
  server: BrowserRunningServerLabelInput,
  projectRootPath?: string | null,
): number {
  const serverPort = browserServerPort(server.url)
  const serverCwd = server.cwd ? normalizePathForBrowserLabel(server.cwd) : null
  const targetCwd = commandWorkingDirectory(target.command, projectRootPath)
  const projectRoot = projectRootPath ? normalizePathForBrowserLabel(projectRootPath) : null
  let score = 0

  if (projectRoot) {
    if (!serverCwd || !pathContainsOrEquals(projectRoot, serverCwd)) {
      return 0
    }
  }

  if (serverPort !== null && commandPorts(target.command).includes(serverPort)) {
    score += 80
  }
  if (serverCwd && targetCwd) {
    if (serverCwd === targetCwd) {
      score += targetCwd === projectRoot ? 20 : 100
    } else if (pathContainsOrEquals(targetCwd, serverCwd)) {
      score += targetCwd === projectRoot ? 20 : 70
    } else {
      const serverLeaf = serverCwd.split("/").filter(Boolean).at(-1)
      const targetLeaf = targetCwd.split("/").filter(Boolean).at(-1)
      if (serverLeaf && targetLeaf && serverLeaf === targetLeaf) score += 35
    }
  }
  if (target.browserSupported === true) score += 5
  return score
}

function bestStartTargetForServer(
  server: BrowserRunningServerLabelInput,
  startTargets: readonly BrowserServerLabelStartTarget[] = [],
  projectRootPath?: string | null,
): BrowserServerLabelStartTarget | null {
  let best: { score: number; target: BrowserServerLabelStartTarget } | null = null
  for (const target of startTargets) {
    if (target.browserSupported !== true) continue
    const score = scoreStartTargetForServer(target, server, projectRootPath)
    if (score < 35) continue
    if (!best || score > best.score) best = { score, target }
  }
  return best?.target ?? null
}

function normalizedTargetName(value?: string | null): string | null {
  const name = value?.trim().toLowerCase()
  return name || null
}

export function browserLaunchTargetMatchesStartTarget(
  target: BrowserLaunchTarget,
  startTarget: BrowserServerLabelStartTarget,
): boolean {
  if (startTarget.browserSupported !== true) return false
  const name = normalizedTargetName(startTarget.name)
  if (!name) return false

  const source = normalizedTargetName(target.source)
  if (source === name) return true

  const label = normalizedTargetName(target.label)
  return label === name || label?.startsWith(`${name} ·`) === true
}

export function browserLaunchTargetMatchesBrowserStartTarget(
  target: BrowserLaunchTarget,
  startTargets: readonly BrowserServerLabelStartTarget[] = [],
): boolean {
  if (startTargets.length === 0) return true
  return startTargets.some((startTarget) =>
    browserLaunchTargetMatchesStartTarget(target, startTarget),
  )
}

export function browserRunningServerDisplayLabel(
  server: BrowserRunningServerLabelInput,
  startTargets: readonly BrowserServerLabelStartTarget[] = [],
  projectRootPath?: string | null,
): string | null {
  const target = bestStartTargetForServer(server, startTargets, projectRootPath)
  if (!target) return null
  return `${target.name} · ${browserLaunchTargetHost(server.url)}`
}
