/** @vitest-environment jsdom */

import { render, screen, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

type ListenerHandle = () => void
type InvokeHandler = (args: Record<string, unknown> | undefined) => unknown

const invokeResponses = new Map<string, InvokeHandler>()
const eventListeners = new Map<string, ((event: { payload: unknown }) => void)[]>()
const invokedCommands: string[] = []

function resetBridge() {
  invokeResponses.clear()
  eventListeners.clear()
  invokedCommands.length = 0
}

function registerInvoke(command: string, handler: InvokeHandler) {
  invokeResponses.set(command, handler)
}

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
  invoke: async (command: string, args?: Record<string, unknown>) => {
    invokedCommands.push(command)
    const handler = invokeResponses.get(command)
    if (!handler) return undefined
    return handler(args)
  },
}))

vi.mock("@tauri-apps/api/event", () => ({
  listen: async (
    eventName: string,
    handler: (event: { payload: unknown }) => void,
  ): Promise<ListenerHandle> => {
    const list = eventListeners.get(eventName) ?? []
    list.push(handler)
    eventListeners.set(eventName, list)
    return () => {
      const existing = eventListeners.get(eventName) ?? []
      eventListeners.set(
        eventName,
        existing.filter((entry) => entry !== handler),
      )
    }
  },
}))

import { SolanaWorkbenchSidebar } from "./solana-workbench-sidebar"

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

function readyToolchain() {
  const present = { present: true, path: "/usr/bin/tool", version: "1.0.0" }
  return {
    solanaCli: present,
    anchor: present,
    cargoBuildSbf: present,
    rust: present,
    node: present,
    pnpm: present,
    surfpool: present,
    trident: present,
    codama: present,
    solanaVerify: present,
    installSupported: true,
    installableComponents: [],
  }
}

function registerDefaultSolanaResponses() {
  registerInvoke("solana_cluster_list", () => [
    {
      kind: "localnet",
      label: "Localnet",
      startable: true,
      defaultRpcUrl: "http://127.0.0.1:8899",
    },
  ])
  registerInvoke("solana_toolchain_status", readyToolchain)
  registerInvoke("solana_toolchain_install_status", () => ({
    inProgress: false,
    managedRoot: "/tmp/solana-tools",
    components: [],
  }))
  registerInvoke("solana_cluster_status", () => ({ running: false }))
  registerInvoke("solana_snapshot_list", () => [])
  registerInvoke("solana_persona_roles", () => [])
  registerInvoke("solana_persona_list", () => [
    {
      name: "alice",
      role: "whale",
      cluster: "localnet",
      pubkey: "Alice1111111111111111111111111111111111111",
      keypairPath: "/tmp/alice.json",
      createdAtMs: 1,
      seed: {},
    },
    {
      name: "bob",
      role: "lp",
      cluster: "localnet",
      pubkey: "Bob111111111111111111111111111111111111111",
      keypairPath: "/tmp/bob.json",
      createdAtMs: 2,
      seed: {},
    },
  ])
  registerInvoke("solana_scenario_list", () => [
    {
      id: "seed-liquidity",
      label: "Seed liquidity",
      description: "Seed pool liquidity",
      supportedClusters: ["localnet"],
      requiredClonePrograms: [],
      requiredRoles: ["whale"],
      kind: "self_contained",
    },
    {
      id: "swap-path",
      label: "Swap path",
      description: "Exercise a swap path",
      supportedClusters: ["localnet"],
      requiredClonePrograms: [],
      requiredRoles: ["lp"],
      kind: "self_contained",
    },
  ])
  registerInvoke("solana_replay_list", () => [])
  registerInvoke("solana_logs_active", () => [])
  registerInvoke("solana_token_extension_matrix", () => ({
    manifestVersion: "1",
    generatedAt: "2026-04-24",
    entries: [],
  }))
  registerInvoke("solana_wallet_scaffold_list", () => [
    {
      kind: "wallet_adapter",
      label: "Wallet Adapter",
      summary: "React wallet adapter scaffold",
      requiresApiKey: false,
      supportedClusters: ["localnet"],
    },
    {
      kind: "wallet_standard",
      label: "Wallet Standard",
      summary: "Wallet Standard scaffold",
      requiresApiKey: false,
      supportedClusters: ["localnet"],
    },
    {
      kind: "privy",
      label: "Privy",
      summary: "Privy scaffold",
      requiresApiKey: true,
      supportedClusters: ["localnet"],
    },
    {
      kind: "dynamic",
      label: "Dynamic",
      summary: "Dynamic scaffold",
      requiresApiKey: true,
      supportedClusters: ["localnet"],
    },
    {
      kind: "mwa_stub",
      label: "Mobile Wallet Adapter",
      summary: "MWA stub scaffold",
      requiresApiKey: false,
      supportedClusters: ["localnet"],
    },
  ])
  registerInvoke("solana_secrets_patterns", () => [])
  registerInvoke("solana_cluster_drift_tracked_programs", () => [])
  registerInvoke("solana_doc_catalog", () => [])
  registerInvoke("solana_subscribe_ready", () => undefined)
}

let storage: Storage | null = null

beforeEach(() => {
  storage = installLocalStorage()
  registerDefaultSolanaResponses()
})

afterEach(() => {
  resetBridge()
  vi.restoreAllMocks()
  storage?.clear()
})

describe("SolanaWorkbenchSidebar", () => {
  it("does not badge static tab inventory counts", async () => {
    render(<SolanaWorkbenchSidebar open />)

    await waitFor(() => {
      expect(invokedCommands).toContain("solana_wallet_scaffold_list")
    })

    expect(screen.getByRole("tab", { name: "Personas" })).toBeVisible()
    expect(screen.getByRole("tab", { name: "Scenarios" })).toBeVisible()
    expect(screen.getByRole("tab", { name: "Wallet" })).toBeVisible()
    expect(
      screen.queryByRole("tab", { name: /Personas, 2/i }),
    ).not.toBeInTheDocument()
    expect(
      screen.queryByRole("tab", { name: /Scenarios, 2/i }),
    ).not.toBeInTheDocument()
    expect(
      screen.queryByRole("tab", { name: /Wallet, 5/i }),
    ).not.toBeInTheDocument()
  })
})
