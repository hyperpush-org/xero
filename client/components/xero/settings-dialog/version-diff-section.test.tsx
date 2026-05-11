import { render, screen, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { VersionDiffSection } from '@/components/xero/settings-dialog/version-diff-section'
import type { AgentDefinitionVersionDiffDto } from '@/src/lib/xero-model/agent-definition'

const everySectionDiff: AgentDefinitionVersionDiffDto = {
  schema: 'xero.agent_definition_version_diff.v1',
  definitionId: 'release_notes_helper',
  fromVersion: 7,
  toVersion: 8,
  fromCreatedAt: '2026-05-01T12:00:00Z',
  toCreatedAt: '2026-05-02T09:30:00Z',
  changed: true,
  changedSections: [
    'identity',
    'prompts',
    'attachedSkills',
    'toolPolicy',
    'memoryPolicy',
    'retrievalPolicy',
    'handoffPolicy',
    'outputContract',
    'databaseAccess',
    'consumedArtifacts',
    'workflowStructure',
    'safetyLimits',
  ],
  sections: [
    {
      section: 'identity',
      changed: true,
      fields: ['displayName'],
      before: { displayName: 'Helper' },
      after: { displayName: 'Release Helper' },
    },
    {
      section: 'prompts',
      changed: true,
      fields: ['prompts'],
      before: { prompts: [{ id: 'old' }] },
      after: { prompts: [{ id: 'new' }] },
    },
    {
      section: 'attachedSkills',
      changed: true,
      fields: ['attachedSkills'],
      before: { attachedSkills: [] },
      after: {
        attachedSkills: [
          { sourceId: 'skill-source:v1:global:bundled:core:rust-best-practices' },
        ],
      },
    },
    {
      section: 'toolPolicy',
      changed: true,
      fields: ['toolPolicy', 'tools'],
      before: {
        toolPolicy: { allowedTools: ['read'] },
        tools: ['read'],
      },
      after: {
        toolPolicy: { allowedTools: ['read', 'project_context_search'] },
        tools: ['read', 'project_context_search'],
      },
    },
    {
      section: 'memoryPolicy',
      changed: true,
      fields: ['memoryPolicy'],
      before: { memoryPolicy: { reviewRequired: false } },
      after: { memoryPolicy: { reviewRequired: true } },
    },
    {
      section: 'retrievalPolicy',
      changed: true,
      fields: ['retrievalDefaults'],
      before: { retrievalDefaults: { enabled: false } },
      after: { retrievalDefaults: { enabled: true, limit: 5 } },
    },
    {
      section: 'handoffPolicy',
      changed: true,
      fields: ['handoffPolicy'],
      before: { handoffPolicy: { enabled: false } },
      after: { handoffPolicy: { enabled: true, preserveDefinitionVersion: true } },
    },
    {
      section: 'outputContract',
      changed: true,
      fields: ['outputContract', 'output'],
      before: { outputContract: 'plain', output: { contract: 'plain' } },
      after: { outputContract: 'structured', output: { contract: 'structured' } },
    },
    {
      section: 'databaseAccess',
      changed: true,
      fields: ['dbTouchpoints'],
      before: { dbTouchpoints: { reads: [], writes: [] } },
      after: { dbTouchpoints: { reads: ['runs'], writes: [] } },
    },
    {
      section: 'consumedArtifacts',
      changed: true,
      fields: ['consumes'],
      before: { consumes: [] },
      after: { consumes: ['plan_pack'] },
    },
    {
      section: 'workflowStructure',
      changed: true,
      fields: ['workflowStructure'],
      before: { workflowStructure: null },
      after: {
        workflowStructure: {
          startPhaseId: 'plan',
          phases: [{ id: 'plan' }],
        },
      },
    },
    {
      section: 'safetyLimits',
      changed: true,
      fields: ['safetyLimits', 'capabilityFlags'],
      before: {
        safetyLimits: { maxIterations: 10 },
        capabilityFlags: [],
      },
      after: {
        safetyLimits: { maxIterations: 20 },
        capabilityFlags: ['allowExternalNetwork'],
      },
    },
  ],
}

describe('VersionDiffSection', () => {
  it('shows an idle hint when no diff has been requested', () => {
    render(
      <VersionDiffSection
        status="idle"
        errorMessage={null}
        diff={null}
        fromVersion={null}
        toVersion={null}
      />,
    )
    expect(
      screen.getByText('Pick two versions above to compare them.'),
    ).toBeInTheDocument()
  })

  it('renders the loading state with the version range in the header', () => {
    const { container } = render(
      <VersionDiffSection
        status="loading"
        errorMessage={null}
        diff={null}
        fromVersion={3}
        toVersion={4}
      />,
    )
    expect(screen.getByText('Loading diff')).toBeInTheDocument()
    expect(container.textContent).toMatch(/v3/)
    expect(container.textContent).toMatch(/v4/)
  })

  it('surfaces an error message when the diff fails to load', () => {
    render(
      <VersionDiffSection
        status="error"
        errorMessage="Backend unavailable."
        diff={null}
        fromVersion={3}
        toVersion={4}
      />,
    )
    expect(screen.getByText('Backend unavailable.')).toBeInTheDocument()
  })

  it('reports an unchanged diff with a friendly hint', () => {
    const unchanged: AgentDefinitionVersionDiffDto = {
      ...everySectionDiff,
      changed: false,
      changedSections: [],
      sections: everySectionDiff.sections.map((section) => ({
        ...section,
        changed: false,
        before: section.before,
        after: section.before,
      })),
    }
    render(
      <VersionDiffSection
        status="ready"
        errorMessage={null}
        diff={unchanged}
        fromVersion={unchanged.fromVersion}
        toVersion={unchanged.toVersion}
      />,
    )
    expect(
      screen.getByText(/byte-equivalent across every diff section/i),
    ).toBeInTheDocument()
    expect(screen.getByText('No changes')).toBeInTheDocument()
  })

  it('renders a labeled bucket for every validator section that changed', () => {
    render(
      <VersionDiffSection
        status="ready"
        errorMessage={null}
        diff={everySectionDiff}
        fromVersion={everySectionDiff.fromVersion}
        toVersion={everySectionDiff.toVersion}
      />,
    )

    const expectedLabels = [
      'Identity',
      'Prompts',
      'Attached skills',
      'Tool policy',
      'Memory policy',
      'Retrieval policy',
      'Handoff policy',
      'Output contract',
      'Database touchpoints',
      'Consumed artifacts',
      'Workflow',
      'Safety limits',
    ]
    for (const label of expectedLabels) {
      expect(screen.getByText(label)).toBeInTheDocument()
    }
    expect(screen.getByText('12 sections changed')).toBeInTheDocument()
  })

  it('shows before/after panes for fields whose values diverge', () => {
    render(
      <VersionDiffSection
        status="ready"
        errorMessage={null}
        diff={everySectionDiff}
        fromVersion={everySectionDiff.fromVersion}
        toVersion={everySectionDiff.toVersion}
      />,
    )

    const beforePanes = screen.getAllByText('Before')
    const afterPanes = screen.getAllByText('After')
    expect(beforePanes.length).toBeGreaterThan(0)
    expect(afterPanes.length).toBe(beforePanes.length)

    const toolPolicyBucket = screen.getByText('Tool policy').closest('li')
    expect(toolPolicyBucket).not.toBeNull()
    if (toolPolicyBucket) {
      const region = within(toolPolicyBucket as HTMLElement)
      expect(region.getByText('toolPolicy')).toBeInTheDocument()
      expect(region.getByText('tools')).toBeInTheDocument()
      expect(region.getAllByText(/project_context_search/).length).toBeGreaterThan(0)
    }
  })
})
