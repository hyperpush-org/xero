export interface BrowserLaunchTarget {
  id: string
  label: string
  url: string
  source?: string | null
  detectedAt: number
}

export interface BrowserLaunchTargetInput {
  label: string
  url: string
  source?: string | null
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

export function browserLaunchTargetLabel(url: string, source?: string | null): string {
  try {
    const parsed = new URL(url)
    const host = parsed.port ? `${parsed.hostname}:${parsed.port}` : parsed.hostname
    return source ? `${source} · ${host}` : host
  } catch {
    return source ? `${source} · local app` : "local app"
  }
}

export function makeBrowserLaunchTarget(input: BrowserLaunchTargetInput): BrowserLaunchTarget | null {
  const url = normalizeBrowserLaunchUrl(input.url)
  if (!url) return null
  const source = input.source?.trim() || null
  return {
    id: browserLaunchTargetId(url),
    label: input.label.trim() || browserLaunchTargetLabel(url, source),
    url,
    source,
    detectedAt: Date.now(),
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
