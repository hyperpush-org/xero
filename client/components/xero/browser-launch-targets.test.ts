import { describe, expect, it } from "vitest"

import {
  browserLaunchTargetLabel,
  browserLaunchTargetMatchesBrowserStartTarget,
  browserLaunchTargetMatchesUrl,
  browserRunningServerDisplayLabel,
  extractBrowserSupportedDevServerUrls,
  isBrowserSupportedDevServerUrl,
  makeBrowserLaunchTarget,
  normalizeLoopbackBrowserUrl,
} from "./browser-launch-targets"

describe("browser launch targets", () => {
  it("extracts local dev-server URLs from terminal output", () => {
    expect(
      extractBrowserSupportedDevServerUrls(
        "\u001b[32mVITE\u001b[0m ready\n  Local: http://localhost:5173/\n  API: http://127.0.0.1:4000/docs",
      ),
    ).toEqual(["http://localhost:5173/", "http://127.0.0.1:4000/docs"])
  })

  it("rejects non-local browser URLs for project launch targets", () => {
    expect(isBrowserSupportedDevServerUrl("https://example.com")).toBe(false)
    expect(isBrowserSupportedDevServerUrl("http://localhost:3000")).toBe(true)
  })

  it("builds stable project browser targets", () => {
    const target = makeBrowserLaunchTarget({
      label: "web",
      url: "http://localhost:5173/",
      source: "vite",
    })

    expect(target).toMatchObject({
      id: "browser-app:http://127.0.0.1:5173/",
      label: "web",
      url: "http://localhost:5173/",
      source: "vite",
    })
  })

  it("renders project target source names exactly as configured", () => {
    expect(browserLaunchTargetLabel("http://localhost:5173/", "web")).toBe(
      "web · localhost:5173",
    )
    expect(browserLaunchTargetLabel("http://localhost:4000/", "api-server")).toBe(
      "api-server · localhost:4000",
    )
  })

  it("labels only browser-supported scanned local servers from configured start targets", () => {
    const targets = [
      {
        name: "web",
        command: "cd apps/web && pnpm dev",
        browserSupported: true,
      },
      {
        name: "api",
        command: "cd api && mix phx.server",
        browserSupported: false,
      },
    ]

    expect(
      browserRunningServerDisplayLabel(
        {
          cwd: "/repo/apps/web",
          processName: "node",
          url: "http://127.0.0.1:3000/",
        },
        targets,
        "/repo",
      ),
    ).toBe("web · 127.0.0.1:3000")

    expect(
      browserRunningServerDisplayLabel(
        {
          cwd: "/repo/api",
          processName: "beam.smp",
          url: "http://127.0.0.1:4000/",
        },
        targets,
        "/repo",
      ),
    ).toBeNull()
  })

  it("does not label scanned servers whose process cwd is outside the active project", () => {
    const targets = [
      {
        name: "web",
        command: "PORT=4100 pnpm dev",
        browserSupported: true,
      },
    ]

    expect(
      browserRunningServerDisplayLabel(
        {
          cwd: "/repo/other-project",
          processName: "node",
          url: "http://127.0.0.1:4100/",
        },
        targets,
        "/repo/active-project",
      ),
    ).toBeNull()

    expect(
      browserRunningServerDisplayLabel(
        {
          cwd: "/repo/active-project",
          processName: "node",
          url: "http://127.0.0.1:4100/",
        },
        targets,
        "/repo/active-project",
      ),
    ).toBe("web · 127.0.0.1:4100")
  })

  it("filters detected terminal targets through browser-supported start target names", () => {
    const targets = [
      { name: "web", command: "cd apps/web && pnpm dev", browserSupported: true },
      { name: "api", command: "cd api && mix phx.server", browserSupported: false },
    ]

    const web = makeBrowserLaunchTarget({
      label: "web · localhost:3000",
      source: "web",
      url: "http://localhost:3000/",
    })
    const api = makeBrowserLaunchTarget({
      label: "api · localhost:4000",
      source: "api",
      url: "http://localhost:4000/",
    })

    expect(web).not.toBeNull()
    expect(api).not.toBeNull()
    expect(browserLaunchTargetMatchesBrowserStartTarget(web!, targets)).toBe(true)
    expect(browserLaunchTargetMatchesBrowserStartTarget(api!, targets)).toBe(false)
  })

  it("matches unavailable project targets by normalized dev-server origin", () => {
    const target = makeBrowserLaunchTarget({
      label: "web",
      url: "http://localhost:5173/",
      source: "vite",
    })

    expect(target).not.toBeNull()
    expect(browserLaunchTargetMatchesUrl(target!, "http://127.0.0.1:5173/dashboard")).toBe(true)
    expect(browserLaunchTargetMatchesUrl(target!, "http://127.0.0.1:5174/")).toBe(false)
  })

  it("preserves localhost while normalizing wildcard loopback hosts for embedded WebViews", () => {
    expect(normalizeLoopbackBrowserUrl("http://localhost:4200/path?q=1")).toBe(
      "http://localhost:4200/path?q=1",
    )
    expect(normalizeLoopbackBrowserUrl("http://0.0.0.0:4200/")).toBe(
      "http://127.0.0.1:4200/",
    )
    expect(normalizeLoopbackBrowserUrl("http://[::1]:4200/")).toBe(
      "http://[::1]:4200/",
    )
  })
})
