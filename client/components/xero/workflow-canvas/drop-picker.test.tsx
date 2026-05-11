import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { DropPicker } from './drop-picker'
import type {
  AgentAuthoringCatalogDto,
  AgentAuthoringProfileAvailabilityDto,
  AgentAuthoringConstraintExplanationDto,
  AgentAuthoringToolCategoryDto,
  AgentAuthoringDbTableDto,
  AgentAuthoringUpstreamArtifactDto,
  AgentToolSummaryDto,
} from '@/src/lib/xero-model/workflow-agents'
import type { AgentDefinitionBaseCapabilityProfileDto } from '@/src/lib/xero-model/agent-definition'

const PROFILES: readonly AgentDefinitionBaseCapabilityProfileDto[] = [
  'observe_only',
  'planning',
  'engineering',
]

const READ_TOOL: AgentToolSummaryDto = {
  name: 'read',
  group: 'observe',
  description: 'Read a file.',
  effectClass: 'observe',
  riskClass: 'observe',
  tags: [],
  schemaFields: [],
  examples: [],
}

const COMMAND_TOOL: AgentToolSummaryDto = {
  name: 'command_run',
  group: 'engineering',
  description: 'Run a shell command.',
  effectClass: 'command',
  riskClass: 'command',
  tags: [],
  schemaFields: [],
  examples: [],
}

const PLAN_PACK_TOOL: AgentToolSummaryDto = {
  name: 'plan_pack_writer',
  group: 'planning',
  description: 'Write a plan pack.',
  effectClass: 'write',
  riskClass: 'workspace_write',
  tags: [],
  schemaFields: [],
  examples: [],
}

// `engineering_only` category: zero tools available on observe_only,
// requires the engineering profile to unlock.
// `mixed` category: one observe tool (always available) plus one command tool
// (gated by engineering). Exercises the "partial" status branch.
const TOOL_CATEGORIES: AgentAuthoringToolCategoryDto[] = [
  {
    id: 'engineering_only',
    label: 'Engineering only',
    description: 'Mutating tools.',
    tools: [COMMAND_TOOL],
  },
  {
    id: 'mixed',
    label: 'Mixed',
    description: 'Observe + command.',
    tools: [READ_TOOL, COMMAND_TOOL],
  },
  {
    id: 'observe_only',
    label: 'Observe only',
    description: 'Read-only.',
    tools: [READ_TOOL],
  },
]

const DB_TABLES: AgentAuthoringDbTableDto[] = [
  {
    table: 'project_records',
    purpose: 'Stores curated project records.',
    columns: ['id', 'kind', 'body'],
  },
  {
    table: 'agent_definitions',
    purpose: 'Custom agent definitions (engineering touchpoint).',
    columns: ['id', 'snapshot'],
  },
]

const UPSTREAM_ARTIFACTS: AgentAuthoringUpstreamArtifactDto[] = [
  {
    sourceAgent: 'plan',
    sourceAgentLabel: 'Plan',
    contract: 'plan_pack',
    contractLabel: 'Plan pack',
    label: 'Plan output',
    description: 'A plan pack handoff.',
    sections: [],
  },
  {
    sourceAgent: 'engineer',
    sourceAgentLabel: 'Engineer',
    contract: 'engineering_summary',
    contractLabel: 'Engineering summary',
    label: 'Eng summary',
    description: 'An engineering summary.',
    sections: [],
  },
]

function profileAvailability(
  subjectKind: string,
  subjectId: string,
  // Profiles on which the subject is exposed.
  allowed: readonly AgentDefinitionBaseCapabilityProfileDto[],
): AgentAuthoringProfileAvailabilityDto[] {
  const requiredProfile = allowed[0] ?? null
  return PROFILES.map((profile) => {
    if (allowed.includes(profile)) {
      return {
        subjectKind,
        subjectId,
        baseCapabilityProfile: profile,
        status: 'available',
        reason: `${subjectKind} is available for this base capability profile.`,
      }
    }
    if (requiredProfile) {
      return {
        subjectKind,
        subjectId,
        baseCapabilityProfile: profile,
        status: 'requires_profile_change',
        reason: `${subjectKind} requires the \`${requiredProfile}\` base capability profile.`,
        requiredProfile,
      }
    }
    return {
      subjectKind,
      subjectId,
      baseCapabilityProfile: profile,
      status: 'unavailable',
      reason: `${subjectKind} is not exposed by any current runtime profile.`,
    }
  })
}

function constraintExplanationsFor(
  availability: readonly AgentAuthoringProfileAvailabilityDto[],
): AgentAuthoringConstraintExplanationDto[] {
  return availability
    .filter((entry) => entry.status !== 'available')
    .map((entry) => ({
      id: `${entry.subjectKind}:${entry.subjectId}:${entry.baseCapabilityProfile}`,
      subjectKind: entry.subjectKind,
      subjectId: entry.subjectId,
      baseCapabilityProfile: entry.baseCapabilityProfile,
      status: entry.status,
      message: `${entry.subjectKind} ${entry.subjectId} blocked on ${entry.baseCapabilityProfile}.`,
      resolution:
        entry.status === 'requires_profile_change' && entry.requiredProfile
          ? `Switch to ${entry.requiredProfile} or remove ${entry.subjectId}.`
          : `Remove ${entry.subjectId} or enable a runtime that exposes it.`,
      requiredProfile: entry.requiredProfile ?? null,
      source: 'profileAvailability',
    }))
}

function buildCatalog(): AgentAuthoringCatalogDto {
  const availability = [
    ...profileAvailability('tool', READ_TOOL.name, ['observe_only', 'planning', 'engineering']),
    ...profileAvailability('tool', COMMAND_TOOL.name, ['engineering']),
    ...profileAvailability('tool', PLAN_PACK_TOOL.name, ['planning']),
    ...profileAvailability('db_touchpoint', 'project_records', [
      'observe_only',
      'planning',
      'engineering',
    ]),
    ...profileAvailability('db_touchpoint', 'agent_definitions', ['engineering']),
    ...profileAvailability('upstream_artifact', 'plan:plan_pack', ['planning', 'engineering']),
    ...profileAvailability('upstream_artifact', 'engineer:engineering_summary', ['engineering']),
  ]
  return {
    contractVersion: 1,
    tools: [READ_TOOL, COMMAND_TOOL, PLAN_PACK_TOOL],
    toolCategories: TOOL_CATEGORIES,
    dbTables: DB_TABLES,
    upstreamArtifacts: UPSTREAM_ARTIFACTS,
    attachableSkills: [],
    policyControls: [],
    templates: [],
    creationFlows: [],
    profileAvailability: availability,
    constraintExplanations: constraintExplanationsFor(availability),
    diagnostics: [],
  }
}

describe('DropPicker profile-aware filtering', () => {
  it('disables tool categories whose tools all require a different profile', () => {
    const onSelect = vi.fn()
    render(
      <DropPicker
        kind="tool-category"
        screenX={0}
        screenY={0}
        catalog={buildCatalog()}
        currentProfile="observe_only"
        onSelectToolCategory={onSelect}
        onClose={vi.fn()}
      />,
    )

    const blocked = screen.getByTestId('drop-picker-category-engineering_only')
    expect(blocked.getAttribute('data-availability')).toBe('requires_profile_change')
    expect(blocked.getAttribute('data-disabled')).toBe('true')

    fireEvent.click(blocked)
    expect(onSelect).not.toHaveBeenCalled()

    const ok = screen.getByTestId('drop-picker-category-observe_only')
    expect(ok.getAttribute('data-availability')).toBe('available')
    expect(ok.getAttribute('data-disabled')).not.toBe('true')

    fireEvent.click(ok)
    expect(onSelect).toHaveBeenCalledWith('observe_only')
  })

  it('marks partially-available categories without disabling them', () => {
    const onSelect = vi.fn()
    render(
      <DropPicker
        kind="tool-category"
        screenX={0}
        screenY={0}
        catalog={buildCatalog()}
        currentProfile="observe_only"
        onSelectToolCategory={onSelect}
        onClose={vi.fn()}
      />,
    )

    const partial = screen.getByTestId('drop-picker-category-mixed')
    expect(partial.getAttribute('data-availability')).toBe('partial')
    expect(partial.getAttribute('data-disabled')).not.toBe('true')
    expect(partial.textContent ?? '').toContain('1 of 2 tools available')

    fireEvent.click(partial)
    expect(onSelect).toHaveBeenCalledWith('mixed')
  })

  it('disables db tables not exposed on the current profile', () => {
    const onSelect = vi.fn()
    render(
      <DropPicker
        kind="db-table"
        screenX={0}
        screenY={0}
        catalog={buildCatalog()}
        currentProfile="planning"
        onSelectDbTable={onSelect}
        onClose={vi.fn()}
      />,
    )

    const gated = screen.getByTestId('drop-picker-db-agent_definitions')
    expect(gated.getAttribute('data-availability')).toBe('requires_profile_change')
    expect(gated.getAttribute('data-disabled')).toBe('true')
    fireEvent.click(gated)
    expect(onSelect).not.toHaveBeenCalled()

    const ok = screen.getByTestId('drop-picker-db-project_records')
    expect(ok.getAttribute('data-availability')).toBe('available')
    fireEvent.click(ok)
    expect(onSelect).toHaveBeenCalledWith('project_records')
  })

  it('disables upstream artifacts gated by profile and uses single-colon subject id', () => {
    const onSelect = vi.fn()
    render(
      <DropPicker
        kind="consumed-artifact"
        screenX={0}
        screenY={0}
        catalog={buildCatalog()}
        currentProfile="planning"
        onSelectConsumedArtifact={onSelect}
        onClose={vi.fn()}
      />,
    )

    const blocked = screen.getByTestId('drop-picker-upstream-engineer-engineering_summary')
    expect(blocked.getAttribute('data-availability')).toBe('requires_profile_change')
    expect(blocked.getAttribute('data-disabled')).toBe('true')

    const ok = screen.getByTestId('drop-picker-upstream-plan-plan_pack')
    expect(ok.getAttribute('data-availability')).toBe('available')
    fireEvent.click(ok)
    expect(onSelect).toHaveBeenCalledWith('plan::plan_pack')
  })

  it('does not filter when no current profile is supplied', () => {
    const onSelect = vi.fn()
    render(
      <DropPicker
        kind="tool-category"
        screenX={0}
        screenY={0}
        catalog={buildCatalog()}
        currentProfile={null}
        onSelectToolCategory={onSelect}
        onClose={vi.fn()}
      />,
    )

    const previouslyBlocked = screen.getByTestId('drop-picker-category-engineering_only')
    expect(previouslyBlocked.getAttribute('data-availability')).toBe('available')
    expect(previouslyBlocked.getAttribute('data-disabled')).not.toBe('true')
    fireEvent.click(previouslyBlocked)
    expect(onSelect).toHaveBeenCalledWith('engineering_only')
  })

  it('counts available db tables per profile', () => {
    function visibleDbCount(profile: AgentDefinitionBaseCapabilityProfileDto): number {
      const { unmount } = render(
        <DropPicker
          kind="db-table"
          screenX={0}
          screenY={0}
          catalog={buildCatalog()}
          currentProfile={profile}
          onSelectDbTable={vi.fn()}
          onClose={vi.fn()}
        />,
      )
      const enabled = DB_TABLES.filter((table) => {
        const node = screen.getByTestId(`drop-picker-db-${table.table}`)
        return node.getAttribute('data-availability') === 'available'
      }).length
      unmount()
      return enabled
    }

    expect(visibleDbCount('observe_only')).toBe(1)
    expect(visibleDbCount('planning')).toBe(1)
    expect(visibleDbCount('engineering')).toBe(2)
  })
})
