import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { ToolHarness } from './index'

const invokeMock = vi.fn<(command: string, args?: unknown) => Promise<unknown>>()
const isTauriMock = vi.fn(() => true)

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (command: string, args?: unknown) => invokeMock(command, args),
  isTauri: () => isTauriMock(),
}))

const sampleCatalog = {
  hostOs: 'macos',
  hostOsLabel: 'macOS',
  skillToolEnabled: true,
  entries: [
    {
      toolName: 'read',
      group: 'core',
      description: 'Read a repo-relative file as text.',
      tags: ['file', 'read'],
      schemaFields: ['path'],
      examples: ['Read src/lib.rs.'],
      riskClass: 'observe',
      effectClass: 'observe',
      runtimeAvailable: true,
      allowedRuntimeAgents: ['engineer', 'plan'],
      activationGroups: ['core'],
      toolPacks: [],
      inputSchema: {
        type: 'object',
        required: ['path'],
        properties: {
          path: { type: 'string', description: 'Repo-relative path.' },
        },
      },
    },
    {
      toolName: 'write',
      group: 'mutation',
      description: 'Create or overwrite a file.',
      tags: ['file', 'write'],
      schemaFields: ['path', 'content'],
      examples: [],
      riskClass: 'write',
      effectClass: 'write',
      runtimeAvailable: true,
      allowedRuntimeAgents: ['engineer'],
      activationGroups: ['mutation'],
      toolPacks: [],
      inputSchema: {
        type: 'object',
        required: ['path', 'content'],
        properties: {
          path: { type: 'string' },
          content: { type: 'string' },
        },
      },
    },
  ],
}

const sampleHarnessProject = {
  projectId: 'xero-developer-tool-harness-fixture',
  displayName: 'Tool harness fixture',
  rootPath: '/tmp/harness-fixture',
}

const successfulRun = {
  runId: 'run-123',
  agentSessionId: 'session-1',
  stoppedEarly: false,
  hadFailure: false,
  results: [
    {
      toolCallId: 'call-1',
      toolName: 'read',
      ok: true,
      summary: 'Read README.md',
      output: { bytes: 42 },
    },
  ],
}

beforeEach(() => {
  invokeMock.mockReset()
  isTauriMock.mockReset()
  isTauriMock.mockReturnValue(true)
  invokeMock.mockImplementation(async (command) => {
    switch (command) {
      case 'developer_tool_catalog':
        return sampleCatalog
      case 'developer_tool_harness_project':
        return sampleHarnessProject
      case 'developer_tool_synthetic_run':
        return successfulRun
      default:
        throw new Error(`unhandled invoke call: ${command}`)
    }
  })
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('ToolHarness', () => {
  it('renders the catalog list with badge counts and tool names', async () => {
    render(<ToolHarness />)
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('developer_tool_catalog', {
        request: { skillToolEnabled: true },
      }),
    )
    expect(await screen.findByText('2 tools')).toBeInTheDocument()
    expect(await screen.findByText('read')).toBeInTheDocument()
    expect(await screen.findByText('write')).toBeInTheDocument()
  })

  it('shows the auto-provisioned harness fixture project', async () => {
    render(<ToolHarness />)
    expect(
      await screen.findByText(/Harness fixture: Tool harness fixture/),
    ).toBeInTheDocument()
    const harnessCalls = invokeMock.mock.calls.filter(
      ([command]) => command === 'developer_tool_harness_project',
    )
    expect(harnessCalls.length).toBeGreaterThan(0)
    const listProjectsCalls = invokeMock.mock.calls.filter(
      ([command]) => command === 'list_projects',
    )
    expect(listProjectsCalls).toHaveLength(0)
  })

  it('renders a Run button per tool', async () => {
    render(<ToolHarness />)
    const runButtons = await screen.findAllByRole('button', { name: /^Run$/ })
    expect(runButtons).toHaveLength(2)
  })

  it('disables Run buttons when the harness fixture cannot be provisioned', async () => {
    invokeMock.mockImplementation(async (command) => {
      switch (command) {
        case 'developer_tool_catalog':
          return sampleCatalog
        case 'developer_tool_harness_project':
          throw new Error('fixture seed failed')
        default:
          throw new Error(`unhandled invoke call: ${command}`)
      }
    })

    render(<ToolHarness />)
    const runButtons = await screen.findAllByRole('button', { name: /^Run$/ })
    await waitFor(() => runButtons.forEach((button) => expect(button).toBeDisabled()))
    expect(await screen.findByText(/fixture seed failed/)).toBeInTheDocument()
  })

  it('runs the tool with synthesised defaults on click and shows the result', async () => {
    render(<ToolHarness />)

    const runButtons = await screen.findAllByRole('button', { name: /^Run$/ })
    fireEvent.click(runButtons[0])

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('developer_tool_synthetic_run', {
        request: {
          projectId: 'xero-developer-tool-harness-fixture',
          calls: [{ toolName: 'read', input: { path: '' } }],
          options: {
            stopOnFailure: true,
            approveWrites: false,
            operatorApproveAll: false,
          },
        },
      }),
    )

    expect(await screen.findByText('Success')).toBeInTheDocument()
    expect(screen.getByText('Read README.md')).toBeInTheDocument()
  })

  it('surfaces an inline error when the run fails', async () => {
    invokeMock.mockImplementation(async (command) => {
      switch (command) {
        case 'developer_tool_catalog':
          return sampleCatalog
        case 'developer_tool_harness_project':
          return sampleHarnessProject
        case 'developer_tool_synthetic_run':
          throw new Error('boom')
        default:
          throw new Error(`unhandled invoke call: ${command}`)
      }
    })

    render(<ToolHarness />)
    const runButtons = await screen.findAllByRole('button', { name: /^Run$/ })
    fireEvent.click(runButtons[0])

    expect(await screen.findByText('boom')).toBeInTheDocument()
    expect(screen.getByText('Error')).toBeInTheDocument()
  })
})
