export const releasePageUrl = "https://github.com/hyperpush-org/xero/releases/latest"
const releaseApiUrl = "https://api.github.com/repos/hyperpush-org/xero/releases/latest"

const assetPatterns = {
  "macos-apple-silicon": /^Xero_.*_aarch64_macos-aarch64\.dmg$/,
  windows: /^Xero_.*_x64-setup\.exe$/,
  linux: /^Xero_.*_amd64\.AppImage$/,
} as const

export type DownloadTarget = keyof typeof assetPatterns
export type UnsupportedDownloadTarget = "macos-intel"

export const unsupportedDownloadUrls = {
  "macos-intel": "/download/unsupported/macos-intel",
} as const satisfies Record<UnsupportedDownloadTarget, string>

type GitHubRelease = {
  html_url?: string
  assets?: Array<{
    name: string
    browser_download_url?: string
  }>
}

function normalizeHint(value: string | null) {
  return value?.replace(/^"|"$/g, "").toLowerCase() ?? ""
}

function getRequestPlatform(headers: Headers) {
  const platform = normalizeHint(headers.get("sec-ch-ua-platform"))
  const architecture = normalizeHint(headers.get("sec-ch-ua-arch"))
  const userAgent = headers.get("user-agent")?.toLowerCase() ?? ""

  return { platform, architecture, userAgent }
}

function isMacRequest(platform: string, userAgent: string) {
  return platform.includes("mac") || /macintosh|mac os x/.test(userAgent)
}

function isIntelArchitecture(architecture: string) {
  return /(x86|x64|amd64|intel)/.test(architecture)
}

export function isDownloadTarget(target: string): target is DownloadTarget {
  return target in assetPatterns
}

export function isUnsupportedDownloadTarget(target: string): target is UnsupportedDownloadTarget {
  return target in unsupportedDownloadUrls
}

export function detectUnsupportedDownloadTarget(headers: Headers): UnsupportedDownloadTarget | null {
  const { platform, architecture, userAgent } = getRequestPlatform(headers)

  if (isMacRequest(platform, userAgent) && isIntelArchitecture(architecture)) {
    return "macos-intel"
  }

  return null
}

export function detectDownloadTarget(headers: Headers): DownloadTarget | null {
  const { platform, userAgent } = getRequestPlatform(headers)

  if (isMacRequest(platform, userAgent)) {
    return "macos-apple-silicon"
  }

  if (platform.includes("windows") || userAgent.includes("windows")) {
    return "windows"
  }

  if (platform.includes("linux") || /linux|x11/.test(userAgent)) {
    return "linux"
  }

  return null
}

export async function resolveDownloadUrl(target: DownloadTarget | null) {
  if (!target) {
    return releasePageUrl
  }

  try {
    const response = await fetch(releaseApiUrl, {
      headers: {
        Accept: "application/vnd.github+json",
        "User-Agent": "xero-landing",
      },
      next: { revalidate: 300 },
    })

    if (!response.ok) {
      return releasePageUrl
    }

    const release = (await response.json()) as GitHubRelease
    const asset = release.assets?.find((candidate) => assetPatterns[target].test(candidate.name))

    return asset?.browser_download_url ?? release.html_url ?? releasePageUrl
  } catch {
    return releasePageUrl
  }
}
