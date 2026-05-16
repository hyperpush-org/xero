import { act, renderHook, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const { invokeMock, isTauriMock, openUrlMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  isTauriMock: vi.fn(() => true),
  openUrlMock: vi.fn(async () => undefined),
}))

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
  isTauri: isTauriMock,
}))

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: openUrlMock,
}))

import { useGitHubAuth } from "./github-auth"

describe("useGitHubAuth", () => {
  beforeEach(() => {
    invokeMock.mockReset()
    isTauriMock.mockReturnValue(true)
    openUrlMock.mockReset()
    openUrlMock.mockResolvedValue(undefined)
  })

  it("loads an existing TUI sign-in from the shared bridge identity", async () => {
    invokeMock.mockResolvedValueOnce({
      signedIn: true,
      account: {
        githubLogin: "octo",
        avatarUrl: "https://avatars.githubusercontent.com/u/1?v=4",
      },
    })

    const { result } = renderHook(() => useGitHubAuth())

    await waitFor(() => expect(result.current.status).toBe("ready"))
    expect(result.current.session?.user.login).toBe("octo")
    expect(invokeMock).toHaveBeenCalledWith("bridge_status")
  })

  it("completes login through the remote bridge poll command", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "bridge_status") {
        return { signedIn: false, account: null }
      }
      if (command === "bridge_sign_in") {
        return {
          signedIn: false,
          authorizationUrl: "https://github.com/login/oauth/authorize?state=flow",
          flowId: "flow",
        }
      }
      if (command === "bridge_poll_github_login") {
        return {
          signedIn: true,
          account: {
            githubLogin: "mona",
            avatarUrl: "https://avatars.githubusercontent.com/u/2?v=4",
          },
        }
      }
      throw new Error(`unexpected command ${command}`)
    })

    const { result } = renderHook(() => useGitHubAuth())
    await waitFor(() => expect(result.current.status).toBe("idle"))

    await act(async () => {
      await result.current.login()
    })

    await waitFor(() => expect(result.current.status).toBe("ready"))
    expect(result.current.session?.user.login).toBe("mona")
    expect(openUrlMock).toHaveBeenCalledWith(
      "https://github.com/login/oauth/authorize?state=flow",
    )
    expect(invokeMock).toHaveBeenCalledWith("bridge_poll_github_login", {
      request: { flowId: "flow" },
    })
  })
})
