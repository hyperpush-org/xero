/** @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

type InvokeHandler = (args: Record<string, unknown> | undefined) => unknown

const invokeResponses = new Map<string, InvokeHandler>()
const invoked: Array<{ command: string; args?: Record<string, unknown> }> = []

function registerInvoke(command: string, handler: InvokeHandler) {
  invokeResponses.set(command, handler)
}

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
  invoke: async (command: string, args?: Record<string, unknown>) => {
    invoked.push({ command, args })
    const handler = invokeResponses.get(command)
    if (!handler) return undefined
    return handler(args)
  },
}))

import { SolanaRpcSection } from "./solana-rpc-section"

function profileResponse() {
  return {
    profiles: [
      {
        id: "paid-devnet",
        cluster: "devnet",
        label: "Paid devnet",
        provider: "helius",
        rpcUrl: "https://rpc.example.test/?api-key=redacted",
        websocketUrl: null,
        secretPlacement: "query_parameter",
        secretName: "api-key",
        hasSecret: true,
        priority: 0,
        enabled: true,
        allowPublicFallback: false,
        rateLimit: null,
        managed: false,
        selected: false,
      },
      {
        id: "builtin-localnet",
        cluster: "localnet",
        label: "Local validator",
        provider: "localnet",
        rpcUrl: "http://127.0.0.1:8899/",
        websocketUrl: "ws://127.0.0.1:8900/",
        secretPlacement: "none",
        secretName: null,
        hasSecret: false,
        priority: 0,
        enabled: true,
        allowPublicFallback: true,
        rateLimit: null,
        managed: true,
        selected: true,
      },
    ],
    selectedProfileIds: { localnet: "builtin-localnet" },
    inventory: [],
  }
}

describe("SolanaRpcSection", () => {
  beforeEach(() => {
    invokeResponses.clear()
    invoked.length = 0
    registerInvoke("solana_provider_profiles_list", profileResponse)
  })

  afterEach(() => {
    cleanup()
  })

  it("loads redacted provider profiles in settings", async () => {
    render(<SolanaRpcSection />)

    expect(await screen.findByText("Paid devnet")).toBeVisible()
    expect(screen.getByText("https://rpc.example.test/?api-key=redacted")).toBeVisible()
    expect(screen.getByText("Local validator")).toBeVisible()
    expect(screen.queryByText("secret-token")).not.toBeInTheDocument()
  })

  it("hides edit and delete actions for built-in profiles", async () => {
    render(<SolanaRpcSection />)

    expect(await screen.findByText("Local validator")).toBeVisible()
    expect(screen.queryByRole("button", { name: "Edit Local validator" })).not.toBeInTheDocument()
    expect(screen.queryByRole("button", { name: "Delete Local validator" })).not.toBeInTheDocument()
    expect(screen.getByRole("button", { name: "Edit Paid devnet" })).toBeVisible()
    expect(screen.getByRole("button", { name: "Delete Paid devnet" })).toBeVisible()
  })

  it("selects a profile from the settings tab", async () => {
    let selectArgs: Record<string, unknown> | undefined
    registerInvoke("solana_provider_profile_select", (args) => {
      selectArgs = args
      return {
        ...profileResponse(),
        selectedProfileIds: { localnet: "builtin-localnet", devnet: "paid-devnet" },
      }
    })

    render(<SolanaRpcSection />)

    fireEvent.click(await screen.findByRole("button", { name: "Select Paid devnet" }))

    await waitFor(() => {
      expect(selectArgs).toEqual({
        request: { cluster: "devnet", profileId: "paid-devnet" },
      })
    })
  })

  it("saves a provider profile from the settings form", async () => {
    let upsertArgs: Record<string, unknown> | undefined
    registerInvoke("solana_provider_profile_upsert", (args) => {
      upsertArgs = args
      return profileResponse()
    })

    render(<SolanaRpcSection />)

    await screen.findByText("Paid devnet")
    fireEvent.click(screen.getByRole("button", { name: "New profile" }))
    expect(await screen.findByRole("dialog")).toBeVisible()
    fireEvent.change(screen.getByLabelText("Display name"), {
      target: { value: "Custom localnet" },
    })
    fireEvent.change(screen.getByLabelText("RPC URL"), {
      target: { value: "http://127.0.0.1:8899" },
    })
    fireEvent.click(screen.getByRole("button", { name: "Save profile" }))

    await waitFor(() => {
      expect(upsertArgs).toMatchObject({
        request: {
          profile: {
            id: "custom-localnet",
            cluster: "localnet",
            label: "Custom localnet",
            provider: "custom",
            rpcUrl: "http://127.0.0.1:8899",
            secretPlacement: "none",
            allowPublicFallback: true,
            enabled: true,
          },
        },
      })
    })
  })

  it("disables saving until required configuration is present", async () => {
    render(<SolanaRpcSection />)

    await screen.findByText("Paid devnet")
    fireEvent.click(screen.getByRole("button", { name: "New profile" }))
    expect(await screen.findByRole("dialog")).toBeVisible()
    expect(screen.getByRole("button", { name: "Save profile" })).toBeDisabled()
    expect(screen.getByText("RPC URL is required.")).toBeVisible()

    fireEvent.change(screen.getByLabelText("RPC URL"), {
      target: { value: "http://127.0.0.1:8899" },
    })
    expect(screen.getByRole("button", { name: "Save profile" })).toBeEnabled()
  })

  it("derives provider auth settings for Helius", async () => {
    let upsertArgs: Record<string, unknown> | undefined
    registerInvoke("solana_provider_profile_upsert", (args) => {
      upsertArgs = args
      return profileResponse()
    })

    render(<SolanaRpcSection />)

    await screen.findByText("Paid devnet")
    fireEvent.click(screen.getByRole("button", { name: "New profile" }))
    expect(await screen.findByRole("dialog")).toBeVisible()
    fireEvent.click(screen.getByLabelText("Provider"))
    fireEvent.click(await screen.findByRole("option", { name: "Helius" }))
    fireEvent.change(screen.getByLabelText("RPC URL"), {
      target: { value: "https://devnet.helius-rpc.com" },
    })
    expect(screen.getByRole("button", { name: "Save profile" })).toBeDisabled()
    expect(screen.getByText("API key is required for Helius.")).toBeVisible()

    fireEvent.change(screen.getByLabelText("API key"), {
      target: { value: "secret-token" },
    })
    expect(screen.getByRole("button", { name: "Save profile" })).toBeEnabled()
    fireEvent.click(screen.getByRole("button", { name: "Save profile" }))

    await waitFor(() => {
      expect(upsertArgs).toMatchObject({
        request: {
          profile: {
            id: "helius-localnet",
            label: "Helius localnet",
            provider: "helius",
            secretPlacement: "query_parameter",
            secretName: "api-key",
            apiKey: "secret-token",
          },
        },
      })
    })
  })

  it("opens custom profiles in the shared edit dialog", async () => {
    render(<SolanaRpcSection />)

    fireEvent.click(await screen.findByRole("button", { name: "Edit Paid devnet" }))

    expect(await screen.findByRole("dialog")).toBeVisible()
    expect(screen.getByRole("heading", { name: "Edit Solana RPC profile" })).toBeVisible()
    expect(screen.getByLabelText("Display name")).toHaveValue("Paid devnet")
    expect(screen.getByLabelText("RPC URL")).toHaveValue(
      "https://rpc.example.test/?api-key=redacted",
    )
  })
})
