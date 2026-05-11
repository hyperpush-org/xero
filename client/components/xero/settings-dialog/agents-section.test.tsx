import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { AgentsSection } from '@/components/xero/settings-dialog/agents-section'
import type {
  AgentDefinitionSummaryDto,
  AgentDefinitionVersionDiffDto,
  AgentDefinitionVersionSummaryDto,
} from '@/src/lib/xero-model/agent-definition'

const builtin: AgentDefinitionSummaryDto = {
  definitionId: 'ask',
  currentVersion: 1,
  displayName: 'Ask',
  shortLabel: 'Ask',
  description: 'Read-only Q&A.',
  scope: 'built_in',
  lifecycleState: 'active',
  baseCapabilityProfile: 'observe_only',
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
  isBuiltIn: true,
}

const projectCustom: AgentDefinitionSummaryDto = {
  definitionId: 'project_research',
  currentVersion: 3,
  displayName: 'Project Research',
  shortLabel: 'Research',
  description: 'Project-aware observe-only researcher.',
  scope: 'project_custom',
  lifecycleState: 'active',
  baseCapabilityProfile: 'observe_only',
  createdAt: '2026-04-30T18:00:00Z',
  updatedAt: '2026-05-01T09:00:00Z',
  isBuiltIn: false,
}

const archivedCustom: AgentDefinitionSummaryDto = {
  ...projectCustom,
  definitionId: 'archived_helper',
  displayName: 'Archived Helper',
  lifecycleState: 'archived',
}

describe('AgentsSection', () => {
  it('renders an empty state when no project is open', () => {
    render(
      <AgentsSection
        projectId={null}
        projectLabel={null}
        onListAgentDefinitions={vi.fn()}
      />,
    )
    expect(
      screen.getByText('Open a project to inspect or manage agent definitions.'),
    ).toBeInTheDocument()
  })

  it('groups built-in and project agents and disables archive on built-ins', async () => {
    const list = vi.fn(async () => ({ definitions: [builtin, projectCustom] }))
    const archive = vi.fn()
    const getVersion = vi.fn(async () => null)
    render(
      <AgentsSection
        projectId="project-1"
        projectLabel="Xero"
        onListAgentDefinitions={list}
        onArchiveAgentDefinition={archive}
        onGetAgentDefinitionVersion={getVersion}
      />,
    )

    await waitFor(() => {
      expect(list).toHaveBeenCalledWith({ projectId: 'project-1', includeArchived: false })
    })

    expect(await screen.findByText('Ask')).toBeInTheDocument()
    expect(await screen.findByText('Project Research')).toBeInTheDocument()
    // built-ins do not get an Archive button
    const archiveButtons = screen.queryAllByRole('button', { name: /^Archive$/i })
    expect(archiveButtons).toHaveLength(1)
  })

  it('archives a custom agent and refreshes the list', async () => {
    const list = vi
      .fn<
        (request: { projectId: string; includeArchived: boolean }) => Promise<{
          definitions: AgentDefinitionSummaryDto[]
        }>
      >()
      .mockResolvedValueOnce({ definitions: [projectCustom] })
      .mockResolvedValueOnce({
        definitions: [{ ...projectCustom, lifecycleState: 'archived' }],
      })
    const archive = vi.fn(async () => ({
      ...projectCustom,
      lifecycleState: 'archived' as const,
    }))
    const onChanged = vi.fn()

    render(
      <AgentsSection
        projectId="project-1"
        projectLabel="Xero"
        onListAgentDefinitions={list}
        onArchiveAgentDefinition={archive}
        onRegistryChanged={onChanged}
      />,
    )

    const archiveButton = await screen.findByRole('button', { name: /Archive/i })
    fireEvent.click(archiveButton)

    await waitFor(() => {
      expect(archive).toHaveBeenCalledWith({
        projectId: 'project-1',
        definitionId: 'project_research',
      })
    })
    await waitFor(() => {
      expect(list).toHaveBeenCalledTimes(2)
    })
    expect(onChanged).toHaveBeenCalledTimes(1)
  })

  it('hides archived agents until the include-archived toggle is enabled', async () => {
    const list = vi
      .fn<
        (request: { projectId: string; includeArchived: boolean }) => Promise<{
          definitions: AgentDefinitionSummaryDto[]
        }>
      >()
      .mockResolvedValueOnce({ definitions: [projectCustom] })
      .mockResolvedValueOnce({ definitions: [projectCustom, archivedCustom] })

    render(
      <AgentsSection
        projectId="project-1"
        projectLabel="Xero"
        onListAgentDefinitions={list}
      />,
    )

    await screen.findByText('Project Research')
    expect(screen.queryByText('Archived Helper')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('checkbox', { name: /Include archived/i }))

    await waitFor(() => {
      expect(list).toHaveBeenLastCalledWith({
        projectId: 'project-1',
        includeArchived: true,
      })
    })
    expect(await screen.findByText('Archived Helper')).toBeInTheDocument()
  })

  it('loads version history on demand when the user opens it', async () => {
    const list = vi.fn(async () => ({ definitions: [projectCustom] }))
    const getVersion = vi.fn(async (request: { version: number }) => ({
      definitionId: projectCustom.definitionId,
      version: request.version,
      createdAt: `2026-05-0${request.version}T09:00:00Z`,
      validationStatus: 'valid',
      validationDiagnosticCount: 0,
      snapshot: {},
      validationReport: { status: 'valid', diagnostics: [] },
    }))

    render(
      <AgentsSection
        projectId="project-1"
        projectLabel="Xero"
        onListAgentDefinitions={list}
        onGetAgentDefinitionVersion={getVersion}
      />,
    )

    await screen.findByText('Project Research')
    fireEvent.click(screen.getByRole('button', { name: /Version history/i }))

    await waitFor(() => {
      expect(getVersion).toHaveBeenCalledWith({
        projectId: 'project-1',
        definitionId: projectCustom.definitionId,
        version: projectCustom.currentVersion,
      })
    })
    const headings = await screen.findAllByText(/^Version \d/)
    expect(headings.length).toBeGreaterThanOrEqual(3)
  })

  it('loads a diff between the latest two versions when history opens', async () => {
    const list = vi.fn(async () => ({ definitions: [projectCustom] }))
    const getVersion = vi.fn(async (request: { version: number }) => ({
      definitionId: projectCustom.definitionId,
      version: request.version,
      createdAt: `2026-05-0${request.version}T09:00:00Z`,
      validationStatus: 'valid' as const,
      validationDiagnosticCount: 0,
      snapshot: {},
      validationReport: { status: 'valid' as const, diagnostics: [] },
    }))
    const diff: AgentDefinitionVersionDiffDto = {
      schema: 'xero.agent_definition_version_diff.v1',
      definitionId: projectCustom.definitionId,
      fromVersion: 2,
      toVersion: 3,
      fromCreatedAt: '2026-05-02T09:00:00Z',
      toCreatedAt: '2026-05-03T09:00:00Z',
      changed: true,
      changedSections: ['toolPolicy'],
      sections: [
        {
          section: 'toolPolicy',
          changed: true,
          fields: ['toolPolicy', 'tools'],
          before: {
            toolPolicy: { allowedTools: ['read'] },
            tools: ['read'],
          },
          after: {
            toolPolicy: { allowedTools: ['read', 'web_search'] },
            tools: ['read', 'web_search'],
          },
        },
      ],
    }
    const getDiff = vi.fn(async () => diff)

    render(
      <AgentsSection
        projectId="project-1"
        projectLabel="Xero"
        onListAgentDefinitions={list}
        onGetAgentDefinitionVersion={getVersion}
        onGetAgentDefinitionVersionDiff={getDiff}
      />,
    )

    await screen.findByText('Project Research')
    fireEvent.click(screen.getByRole('button', { name: /Version history/i }))

    await waitFor(() => {
      expect(getDiff).toHaveBeenCalledWith({
        projectId: 'project-1',
        definitionId: projectCustom.definitionId,
        fromVersion: 2,
        toVersion: 3,
      })
    })

    expect(await screen.findByText('Tool policy')).toBeInTheDocument()
    expect(screen.getByText('1 section changed')).toBeInTheDocument()
  })

  it('reloads the diff when the user picks a different version pair', async () => {
    const list = vi.fn(async () => ({ definitions: [projectCustom] }))
    const getVersion = vi.fn(async (request: { version: number }) => ({
      definitionId: projectCustom.definitionId,
      version: request.version,
      createdAt: `2026-05-0${request.version}T09:00:00Z`,
      validationStatus: 'valid' as const,
      validationDiagnosticCount: 0,
      snapshot: {},
      validationReport: { status: 'valid' as const, diagnostics: [] },
    }))
    const diff: AgentDefinitionVersionDiffDto = {
      schema: 'xero.agent_definition_version_diff.v1',
      definitionId: projectCustom.definitionId,
      fromVersion: 1,
      toVersion: 3,
      fromCreatedAt: '2026-05-01T09:00:00Z',
      toCreatedAt: '2026-05-03T09:00:00Z',
      changed: false,
      changedSections: [],
      sections: [
        {
          section: 'identity',
          changed: false,
          fields: ['displayName'],
          before: { displayName: 'Same' },
          after: { displayName: 'Same' },
        },
      ],
    }
    const getDiff = vi.fn(async () => diff)

    render(
      <AgentsSection
        projectId="project-1"
        projectLabel="Xero"
        onListAgentDefinitions={list}
        onGetAgentDefinitionVersion={getVersion}
        onGetAgentDefinitionVersionDiff={getDiff}
      />,
    )

    await screen.findByText('Project Research')
    fireEvent.click(screen.getByRole('button', { name: /Version history/i }))

    await waitFor(() => {
      expect(getDiff).toHaveBeenCalledTimes(1)
    })

    const fromPicker = screen.getByLabelText('From version')
    fireEvent.change(fromPicker, { target: { value: '1' } })

    await waitFor(() => {
      expect(getDiff).toHaveBeenLastCalledWith({
        projectId: 'project-1',
        definitionId: projectCustom.definitionId,
        fromVersion: 1,
        toVersion: 3,
      })
    })
  })
})
