/** @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { SolanaPersonaPanel } from "./solana-persona-panel"
import { SolanaSafetyPanel } from "./solana-safety-panel"
import { SolanaScenarioPanel } from "./solana-scenario-panel"
import { SolanaTxInspector } from "./solana-tx-inspector"
import type {
  ClusterKind,
  FundingReceipt,
  Persona,
  RoleDescriptor,
  ScenarioDescriptor,
} from "@/src/features/solana/use-solana-workbench"

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

const cluster: ClusterKind = "localnet"

const roles: RoleDescriptor[] = [
  {
    id: "whale",
    preset: {
      displayLabel: "Whale",
      description: "Large localnet wallet",
      lamports: 1_000_000_000,
      tokens: [],
      nfts: [],
    },
  },
]

const personas: Persona[] = [
  {
    name: "alice",
    role: "whale",
    cluster,
    pubkey: "Alice1111111111111111111111111111111111111",
    keypairPath: "/tmp/alice.json",
    createdAtMs: 1,
    seed: { solLamports: 1_000_000_000, tokens: [], nfts: [] },
  },
]

const receipt: FundingReceipt = {
  persona: "alice",
  cluster,
  steps: [],
  succeeded: true,
  startedAtMs: 1,
  finishedAtMs: 2,
}

describe("Solana panel actions", () => {
  it("trims persona create arguments before invoking the workbench handler", async () => {
    const onCreate = vi.fn(async () => receipt)

    render(
      <SolanaPersonaPanel
        cluster={cluster}
        personas={[]}
        roles={roles}
        busy={false}
        onRefresh={vi.fn()}
        onCreate={onCreate}
        onDelete={vi.fn()}
        onFund={vi.fn()}
        clusterRunning
      />,
    )

    fireEvent.change(screen.getByLabelText("Persona name"), {
      target: { value: "  alice  " },
    })
    fireEvent.change(screen.getByLabelText("Persona note"), {
      target: { value: "  local whale  " },
    })
    fireEvent.click(screen.getByRole("button", { name: /Create \+ fund/i }))

    await waitFor(() => {
      expect(onCreate).toHaveBeenCalledWith("alice", "whale", "local whale")
    })
  })

  it("dispatches the selected scenario with the active cluster and persona", async () => {
    const onRunScenario = vi.fn(async () => ({
      id: "seed-liquidity",
      cluster,
      persona: "alice",
      status: "succeeded" as const,
      signatures: [],
      steps: [],
      fundingReceipts: [],
      startedAtMs: 1,
      finishedAtMs: 2,
    }))
    const scenarios: ScenarioDescriptor[] = [
      {
        id: "seed-liquidity",
        label: "Seed liquidity",
        description: "Seed a local pool",
        supportedClusters: [cluster],
        requiredClonePrograms: [],
        requiredRoles: ["whale"],
        kind: "self_contained",
      },
    ]

    render(
      <SolanaScenarioPanel
        cluster={cluster}
        personas={personas}
        scenarios={scenarios}
        busy={false}
        lastRun={null}
        clusterRunning
        onRunScenario={onRunScenario}
      />,
    )

    fireEvent.click(screen.getByRole("button", { name: /Run scenario/i }))

    await waitFor(() => {
      expect(onRunScenario).toHaveBeenCalledWith({
        id: "seed-liquidity",
        cluster,
        persona: "alice",
        params: {},
      })
    })
  })

  it("trims simulation bytes and splits priority-fee program ids", async () => {
    const onSimulate = vi.fn(async () => null)
    const onEstimateFee = vi.fn(async () => ({
      samples: [],
      percentiles: [],
      recommendedMicroLamports: 0,
      recommendedPercentile: "median" as const,
      programIds: [],
      source: "fixture",
    }))

    render(
      <SolanaTxInspector
        cluster={cluster}
        clusterRunning
        txBusy={false}
        lastSimulation={null}
        lastExplanation={null}
        onSimulate={onSimulate}
        onExplain={vi.fn()}
        onEstimateFee={onEstimateFee}
      />,
    )

    fireEvent.change(screen.getByPlaceholderText("AQABAv..."), {
      target: { value: "  AQ==  " },
    })
    fireEvent.click(screen.getByRole("button", { name: /^Simulate$/i }))

    await waitFor(() => {
      expect(onSimulate).toHaveBeenCalledWith({
        cluster,
        transactionBase64: "AQ==",
        skipReplaceBlockhash: false,
      })
    })

    fireEvent.click(screen.getByRole("tab", { name: /Priority fee/i }))
    fireEvent.change(screen.getByPlaceholderText("JUP6Lkb..., whirLb..."), {
      target: {
        value:
          " JUP6Lkb1111111111111111111111111111111111,\n whirLb2222222222222222222222222222222222  ",
      },
    })
    fireEvent.click(screen.getByRole("button", { name: /^Estimate$/i }))

    await waitFor(() => {
      expect(onEstimateFee).toHaveBeenCalledWith([
        "JUP6Lkb1111111111111111111111111111111111",
        "whirLb2222222222222222222222222222222222",
      ])
    })
  })

  it("trims safety scan project roots and keeps empty severity nullable", async () => {
    const onScanSecrets = vi.fn(async () => ({
      projectRoot: "/tmp/fixture",
      filesScanned: 1,
      filesSkipped: 0,
      durationMs: 1,
      findings: [],
      blocksDeploy: false,
      patternsApplied: 1,
    }))

    render(
      <SolanaSafetyPanel
        busy={false}
        projectRootDefault=""
        lastSecretScan={null}
        lastScopeCheck={null}
        lastDrift={null}
        lastCost={null}
        trackedPrograms={[]}
        onScanSecrets={onScanSecrets}
        onRunScopeCheck={vi.fn()}
        onCheckDrift={vi.fn()}
        onRefreshCost={vi.fn()}
        onResetCost={vi.fn()}
      />,
    )

    fireEvent.change(screen.getByPlaceholderText("/absolute/path/to/project"), {
      target: { value: "  /tmp/fixture  " },
    })
    fireEvent.click(screen.getByRole("button", { name: /^Scan$/i }))

    await waitFor(() => {
      expect(onScanSecrets).toHaveBeenCalledWith({
        projectRoot: "/tmp/fixture",
        minSeverity: null,
      })
    })
  })
})
