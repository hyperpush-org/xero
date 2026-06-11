import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const { invokeMock, isTauriMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  isTauriMock: vi.fn(() => true),
}))

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
  isTauri: isTauriMock,
}))

import { CloudAccountSection } from "./cloud-account-section"

const linkedDevice = {
  id: "device-web",
  kind: "web",
  name: "Xero Web",
  lastSeen: "2026-05-31T12:00:00Z",
  revokedAt: null,
  userAgent: null,
}

const desktopDevice = {
  id: "device-desktop",
  kind: "desktop",
  name: "Xero Desktop",
  lastSeen: "2026-05-31T12:10:00Z",
  revokedAt: null,
  userAgent: null,
}

describe("CloudAccountSection", () => {
  beforeEach(() => {
    isTauriMock.mockReset()
    isTauriMock.mockReturnValue(true)
    invokeMock.mockReset()
    invokeMock.mockImplementation((command: string) => {
      if (command === "bridge_status") {
        return Promise.resolve({
          signedIn: true,
          account: { githubLogin: "sn0w" },
          devices: [desktopDevice, linkedDevice],
          devicesError: null,
        })
      }
      if (command === "bridge_revoke_device") {
        return Promise.resolve(null)
      }
      return Promise.resolve(null)
    })
  })

  it("requires a second click before unlinking a linked device", async () => {
    render(<CloudAccountSection />)

    expect(await screen.findByText("Xero Web")).toBeVisible()

    fireEvent.click(screen.getByRole("button", { name: "Unlink Xero Web" }))

    expect(invokeMock).not.toHaveBeenCalledWith("bridge_revoke_device", expect.anything())
    expect(screen.getByRole("button", { name: "Confirm unlink Xero Web" })).toHaveTextContent("Unlink")

    fireEvent.click(screen.getByRole("button", { name: "Confirm unlink Xero Web" }))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("bridge_revoke_device", {
        request: { deviceId: "device-web" },
      }),
    )
  })

  it("hides the current desktop app and only shows cloud web connections", async () => {
    render(<CloudAccountSection />)

    expect(await screen.findByText("Xero Web")).toBeVisible()
    expect(screen.queryByText("Xero Desktop")).not.toBeInTheDocument()
    expect(screen.queryByText("Desktop")).not.toBeInTheDocument()
  })

  it("clears the unlink confirmation when the pointer leaves the button", async () => {
    render(<CloudAccountSection />)

    expect(await screen.findByText("Xero Web")).toBeVisible()

    fireEvent.click(screen.getByRole("button", { name: "Unlink Xero Web" }))
    fireEvent.pointerLeave(screen.getByRole("button", { name: "Confirm unlink Xero Web" }))

    expect(screen.getByRole("button", { name: "Unlink Xero Web" })).toBeVisible()
    expect(screen.queryByRole("button", { name: "Confirm unlink Xero Web" })).not.toBeInTheDocument()
    expect(invokeMock).not.toHaveBeenCalledWith("bridge_revoke_device", expect.anything())
  })
})
