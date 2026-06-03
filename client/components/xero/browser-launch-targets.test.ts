import { describe, expect, it } from "vitest"

import {
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
    ).toEqual(["http://127.0.0.1:5173/", "http://127.0.0.1:4000/docs"])
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
      url: "http://127.0.0.1:5173/",
      source: "vite",
    })
  })

  it("normalizes ambiguous loopback hosts to IPv4 for embedded WebViews", () => {
    expect(normalizeLoopbackBrowserUrl("http://localhost:4200/path?q=1")).toBe(
      "http://127.0.0.1:4200/path?q=1",
    )
    expect(normalizeLoopbackBrowserUrl("http://0.0.0.0:4200/")).toBe(
      "http://127.0.0.1:4200/",
    )
    expect(normalizeLoopbackBrowserUrl("http://[::1]:4200/")).toBe(
      "http://[::1]:4200/",
    )
  })
})
