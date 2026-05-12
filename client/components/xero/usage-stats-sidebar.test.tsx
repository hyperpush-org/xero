import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { UsageStatsSidebar, type UsageStatsSidebarProps } from "./usage-stats-sidebar"
import type { ProjectUsageSummaryDto } from "@/src/lib/xero-model/usage"

function makeUsageSummary(): ProjectUsageSummaryDto {
  return {
    projectId: "project-1",
    totals: {
      runCount: 0,
      inputTokens: 0,
      outputTokens: 0,
      totalTokens: 0,
      cacheReadTokens: 0,
      cacheCreationTokens: 0,
      estimatedCostMicros: 0,
    },
    byModel: [],
  }
}

function renderUsageSidebar(overrides: Partial<UsageStatsSidebarProps> = {}) {
  const props: UsageStatsSidebarProps = {
    open: false,
    projectId: "project-1",
    projectName: "Xero",
    summary: makeUsageSummary(),
    onClose: vi.fn(),
    onRefresh: vi.fn(async () => undefined),
    ...overrides,
  }

  return { ...render(<UsageStatsSidebar {...props} />), props }
}

describe("UsageStatsSidebar", () => {
  it("opens the floating panel with the shared right-sidebar animation frame", () => {
    const { props, rerender } = renderUsageSidebar()
    expect(screen.queryByLabelText("Project usage statistics")).not.toBeInTheDocument()

    rerender(<UsageStatsSidebar {...props} open />)

    const panel = screen.getByLabelText("Project usage statistics")
    expect(panel).toHaveAttribute("data-slot", "floating-right-sidebar-panel")
    expect(panel).toHaveClass("gpu-layer")
    expect(document.querySelector('[data-slot="floating-right-sidebar-overlay"]')).toBeInTheDocument()
    expect(screen.getByText("No agent runs recorded for this project yet.")).toBeVisible()
  })
})
