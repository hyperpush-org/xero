/** @vitest-environment jsdom */

import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { SolanaLogFeed } from "./solana-log-feed"
import type {
  LogEntry,
  LogsViewResponse,
} from "@/src/features/solana/use-solana-workbench"

function logEntry(signature: string, options: { ok?: boolean; event?: boolean } = {}): LogEntry {
  return {
    cluster: "localnet",
    signature,
    slot: 42,
    blockTimeS: null,
    rawLogs: [`Program log: ${signature}`],
    programsInvoked: ["Prog111"],
    explanation: {
      ok: options.ok ?? true,
      summary: `${signature} summary`,
      primaryError: null,
      decodedLogs: {
        entries: [],
        programsInvoked: ["Prog111"],
        totalComputeUnits: 0,
      },
      affectedPrograms: ["Prog111"],
      computeUnitsTotal: 0,
    },
    anchorEvents: options.event
      ? [
          {
            programId: "Prog111",
            eventName: "SwapEvent",
            discriminatorHex: "0102030405060708",
            payloadBase64: "",
            payloadBytesLen: 0,
          },
        ]
      : [],
    err: options.ok === false ? { custom: 6000 } : null,
    receivedMs: 100,
  }
}

function logView(overrides: Partial<LogsViewResponse> = {}): LogsViewResponse {
  return {
    cluster: "localnet",
    programIds: [],
    filter: "all",
    order: "newestFirst",
    limit: 25,
    totalAvailable: 3,
    decodedEventCount: 1,
    counts: { all: 3, errors: 1, events: 1 },
    entries: [logEntry("sig-view", { event: true })],
    ...overrides,
  }
}

describe("SolanaLogFeed", () => {
  it("renders Rust-projected feed rows and requests filtered views", async () => {
    const onRefreshView = vi.fn(async () => logView())

    render(
      <SolanaLogFeed
        activeSubscriptions={[]}
        busy={false}
        cluster="localnet"
        decodedEvents={[]}
        feedVersion={1}
        feedView={logView()}
        lastFetch={null}
        onClear={vi.fn()}
        onFetchRecent={vi.fn(async () => null)}
        onRefreshSubscriptions={vi.fn(async () => undefined)}
        onRefreshView={onRefreshView}
        onSubscribe={vi.fn(async () => null)}
        onUnsubscribe={vi.fn(async () => false)}
      />,
    )

    expect(screen.getByText("sig-view")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: /All3/ })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: /Errors1/ })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: /Anchor events1/ })).toBeInTheDocument()

    await waitFor(() =>
      expect(onRefreshView).toHaveBeenCalledWith({
        cluster: "localnet",
        programIds: [],
        filter: "all",
        order: "newestFirst",
        limit: 25,
      }),
    )

    fireEvent.click(screen.getByRole("button", { name: /Errors1/ }))
    await waitFor(() =>
      expect(onRefreshView).toHaveBeenLastCalledWith({
        cluster: "localnet",
        programIds: [],
        filter: "errors",
        order: "newestFirst",
        limit: 25,
      }),
    )
  })
})
