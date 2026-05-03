/** @vitest-environment jsdom */

import { act, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import {
  EmulatorSidebar,
  useThrottledEmulatorFrameSrc,
} from "./emulator-sidebar"
import { IosEmulatorSidebar } from "./ios-emulator-sidebar"
import { AndroidEmulatorSidebar } from "./android-emulator-sidebar"

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

  it("renders open with a positive width and the platform label", () => {
    const { container } = render(<EmulatorSidebar open platform="android" />)
    const aside = container.querySelector("aside")!
    expect(aside.style.width).not.toBe("0px")
    expect(aside.getAttribute("aria-hidden")).toBe("false")
    expect(screen.getByText("Android Emulator")).toBeVisible()
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
  minIntervalMs = 0,
}: {
  enabled?: boolean
  frameSeq: number | null
  minIntervalMs?: number
}) {
  const { frameSrc, settleFrameRequest } = useThrottledEmulatorFrameSrc({
    enabled,
    frameSeq,
    minIntervalMs,
  })

  return (
    <button data-src={frameSrc ?? ""} onClick={settleFrameRequest} type="button">
      settle
    </button>
  )
}

describe("useThrottledEmulatorFrameSrc", () => {
  it("keeps only one custom-scheme frame request in flight", () => {
    const { rerender } = render(<FrameSrcProbe frameSeq={1} />)
    const probe = screen.getByRole("button", { name: "settle" })

    expect(probe).toHaveAttribute("data-src", "emulator://localhost/frame?t=1")

    rerender(<FrameSrcProbe frameSeq={2} />)
    rerender(<FrameSrcProbe frameSeq={3} />)

    expect(probe).toHaveAttribute("data-src", "emulator://localhost/frame?t=1")

    fireEvent.click(probe)

    expect(probe).toHaveAttribute("data-src", "emulator://localhost/frame?t=3")
  })

  it("enforces a minimum interval between frame requests", () => {
    vi.useFakeTimers()
    try {
      const { rerender } = render(<FrameSrcProbe frameSeq={1} minIntervalMs={100} />)
      const probe = screen.getByRole("button", { name: "settle" })

      rerender(<FrameSrcProbe frameSeq={2} minIntervalMs={100} />)
      fireEvent.click(probe)

      expect(probe).toHaveAttribute("data-src", "emulator://localhost/frame?t=1")

      act(() => {
        vi.advanceTimersByTime(100)
      })

      expect(probe).toHaveAttribute("data-src", "emulator://localhost/frame?t=2")
    } finally {
      vi.useRealTimers()
    }
  })

  it("drops the frame src when disabled", () => {
    const { rerender } = render(<FrameSrcProbe frameSeq={1} />)
    const probe = screen.getByRole("button", { name: "settle" })

    expect(probe).toHaveAttribute("data-src", "emulator://localhost/frame?t=1")

    rerender(<FrameSrcProbe enabled={false} frameSeq={2} />)

    expect(probe).toHaveAttribute("data-src", "")
  })
})
