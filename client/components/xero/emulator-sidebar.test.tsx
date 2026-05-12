/** @vitest-environment jsdom */

import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import {
  EmulatorSidebar,
  useThrottledEmulatorFrameSrc,
} from "./emulator-sidebar"
import { IosEmulatorSidebar } from "./ios-emulator-sidebar"
import { AndroidEmulatorSidebar } from "./android-emulator-sidebar"

type FrameSourceLoader = NonNullable<Parameters<typeof useThrottledEmulatorFrameSrc>[0]["loadFrameSource"]>

// jsdom in this project ships a localStorage whose methods aren't functions;
// install a minimal in-memory shim so width persistence has something to call.
function installLocalStorage() {
  const store = new Map<string, string>()
  const shim: Storage = {
    get length() {
      return store.size
    },
    clear() {
      store.clear()
    },
    getItem(key) {
      return store.has(key) ? store.get(key)! : null
    },
    key(index) {
      return Array.from(store.keys())[index] ?? null
    },
    removeItem(key) {
      store.delete(key)
    },
    setItem(key, value) {
      store.set(key, String(value))
    },
  }
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: shim,
  })
  return shim
}

let storage: Storage

beforeEach(() => {
  storage = installLocalStorage()
})

afterEach(() => {
  storage.clear()
})

describe("EmulatorSidebar", () => {
  it("renders closed with zero width and marked hidden", () => {
    const { container } = render(<EmulatorSidebar open={false} platform="android" />)
    const aside = container.querySelector("aside")!
    expect(aside.style.width).toBe("0px")
    expect(aside.getAttribute("aria-hidden")).toBe("true")
  })

  it("renders first open from zero width before expanding to the platform width", async () => {
    const { container } = render(<EmulatorSidebar open platform="android" />)
    const aside = container.querySelector("aside")!
    expect(aside.style.width).toBe("0px")
    await waitFor(() => expect(aside.style.width).not.toBe("0px"))
    expect(aside.getAttribute("aria-hidden")).toBe("false")
    expect(screen.getByText("Android Emulator")).toBeVisible()
  })

  it("keeps the width transition enabled when closing", () => {
    const { container, rerender } = render(
      <EmulatorSidebar open openImmediately platform="ios" />,
    )
    const aside = container.querySelector("aside")!

    expect(aside.style.width).not.toBe("0px")

    rerender(<EmulatorSidebar open={false} openImmediately platform="ios" />)

    expect(aside.style.width).toBe("0px")
    expect(aside.style.transition).toContain("width")
    expect(aside).toHaveClass("sidebar-motion-island")
  })

  it("labels the iOS sidebar when platform=ios", () => {
    render(<EmulatorSidebar open platform="ios" />)
    expect(screen.getByText("iOS Simulator")).toBeVisible()
  })

  it("resizes with ArrowLeft on the resize handle", () => {
    const { container } = render(<EmulatorSidebar open platform="android" />)
    const separator = container.querySelector("[role='separator']") as HTMLElement
    const before = Number(separator.getAttribute("aria-valuenow"))

    fireEvent.keyDown(separator, { key: "ArrowLeft" })

    const after = Number(separator.getAttribute("aria-valuenow"))
    expect(after).toBeGreaterThan(before)
  })

  it("persists width to localStorage under a platform-specific key", () => {
    const { container, unmount } = render(<EmulatorSidebar open platform="ios" />)
    const separator = container.querySelector("[role='separator']") as HTMLElement
    fireEvent.keyDown(separator, { key: "ArrowLeft" })

    const ios = storage.getItem("xero.emulator.ios.width")
    expect(ios).not.toBeNull()
    unmount()

    // Android should not pick up iOS's width (separate keys).
    render(<EmulatorSidebar open platform="android" />)
    const android = storage.getItem("xero.emulator.android.width")
    expect(android).not.toBe(ios)
  })

  it("thin wrappers pass the right platform through", () => {
    const { unmount } = render(<IosEmulatorSidebar open />)
    expect(screen.getByText("iOS Simulator")).toBeVisible()
    unmount()

    render(<AndroidEmulatorSidebar open />)
    expect(screen.getByText("Android Emulator")).toBeVisible()
  })
})

function FrameSrcProbe({
  enabled = true,
  frameSeq,
  loadFrameSource,
  minIntervalMs = 0,
}: {
  enabled?: boolean
  frameSeq: number | null
  loadFrameSource: FrameSourceLoader
  minIntervalMs?: number
}) {
  const { frameSrc, settleFrameRequest } = useThrottledEmulatorFrameSrc({
    enabled,
    frameSeq,
    loadFrameSource,
    minIntervalMs,
  })

  return (
    <button data-src={frameSrc ?? ""} onClick={settleFrameRequest} type="button">
      settle
    </button>
  )
}

function createFrameSourceLoader() {
  const revoked: string[] = []
  const loadFrameSource: FrameSourceLoader = vi.fn(async (seq: number) => {
    const src = `blob:xero-emulator-frame-${seq}`
    return {
      seq,
      src,
      revoke: () => {
        revoked.push(src)
      },
    }
  })
  return { loadFrameSource, revoked }
}

describe("useThrottledEmulatorFrameSrc", () => {
  it("keeps only one IPC frame request in flight", async () => {
    const { loadFrameSource } = createFrameSourceLoader()
    const { rerender } = render(<FrameSrcProbe frameSeq={1} loadFrameSource={loadFrameSource} />)
    const probe = screen.getByRole("button", { name: "settle" })

    await waitFor(() => {
      expect(probe).toHaveAttribute("data-src", "blob:xero-emulator-frame-1")
    })

    rerender(<FrameSrcProbe frameSeq={2} loadFrameSource={loadFrameSource} />)
    rerender(<FrameSrcProbe frameSeq={3} loadFrameSource={loadFrameSource} />)

    expect(probe).toHaveAttribute("data-src", "blob:xero-emulator-frame-1")
    expect(loadFrameSource).toHaveBeenCalledTimes(1)

    fireEvent.click(probe)

    await waitFor(() => {
      expect(probe).toHaveAttribute("data-src", "blob:xero-emulator-frame-3")
    })
    expect(loadFrameSource).toHaveBeenCalledTimes(2)
    expect(loadFrameSource).toHaveBeenLastCalledWith(3)
  })

  it("enforces a minimum interval between frame requests", async () => {
    vi.useFakeTimers()
    try {
      const { loadFrameSource } = createFrameSourceLoader()
      const { rerender } = render(
        <FrameSrcProbe frameSeq={1} loadFrameSource={loadFrameSource} minIntervalMs={100} />,
      )
      const probe = screen.getByRole("button", { name: "settle" })

      await act(async () => undefined)
      expect(probe).toHaveAttribute("data-src", "blob:xero-emulator-frame-1")

      rerender(<FrameSrcProbe frameSeq={2} loadFrameSource={loadFrameSource} minIntervalMs={100} />)
      fireEvent.click(probe)

      expect(probe).toHaveAttribute("data-src", "blob:xero-emulator-frame-1")

      await act(async () => {
        vi.advanceTimersByTime(100)
        await Promise.resolve()
      })

      expect(probe).toHaveAttribute("data-src", "blob:xero-emulator-frame-2")
    } finally {
      vi.useRealTimers()
    }
  })

  it("uses revocable blob URLs instead of custom-scheme frame URLs", async () => {
    const { loadFrameSource, revoked } = createFrameSourceLoader()
    const { rerender } = render(
      <FrameSrcProbe frameSeq={1} loadFrameSource={loadFrameSource} minIntervalMs={0} />,
    )
    const probe = screen.getByRole("button", { name: "settle" })

    await waitFor(() => {
      expect(probe).toHaveAttribute("data-src", "blob:xero-emulator-frame-1")
    })

    for (let seq = 1; seq <= 12; seq += 1) {
      rerender(<FrameSrcProbe frameSeq={seq} loadFrameSource={loadFrameSource} minIntervalMs={0} />)
      fireEvent.click(probe)
      await waitFor(() => {
        expect(probe.getAttribute("data-src") ?? "").toBe(`blob:xero-emulator-frame-${seq}`)
      })
    }

    expect(probe.getAttribute("data-src") ?? "").not.toContain("emulator://")
    expect(revoked).toContain("blob:xero-emulator-frame-1")
  })

  it("drops the frame src when disabled", async () => {
    const { loadFrameSource, revoked } = createFrameSourceLoader()
    const { rerender } = render(<FrameSrcProbe frameSeq={1} loadFrameSource={loadFrameSource} />)
    const probe = screen.getByRole("button", { name: "settle" })

    await waitFor(() => {
      expect(probe).toHaveAttribute("data-src", "blob:xero-emulator-frame-1")
    })

    rerender(<FrameSrcProbe enabled={false} frameSeq={2} loadFrameSource={loadFrameSource} />)

    expect(probe).toHaveAttribute("data-src", "")
    expect(revoked).toContain("blob:xero-emulator-frame-1")
  })
})
