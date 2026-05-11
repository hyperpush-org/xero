import { render } from '@testing-library/react'
import { ReactFlowProvider } from '@xyflow/react'
import { describe, expect, it } from 'vitest'

import { StageNode } from './stage-node'
import type { StageFlowNode } from '../build-agent-graph'

function stageNode(
  overrides: Partial<StageFlowNode['data']['phase']> = {},
  flags: Partial<Pick<StageFlowNode['data'], 'isStart'>> = {},
): StageFlowNode {
  return {
    id: 'workflow-phase:gather',
    type: 'stage',
    position: { x: 0, y: 0 },
    data: {
      isStart: flags.isStart ?? false,
      phase: {
        id: 'gather',
        title: 'Gather',
        description: 'Pull approved release records.',
        allowedTools: ['read', 'search'],
        requiredChecks: [{ kind: 'tool_succeeded', toolName: 'read' }],
        branches: [],
        ...overrides,
      },
    },
  }
}

function renderNode(node: StageFlowNode) {
  return render(
    <ReactFlowProvider>
      <StageNode
        id={node.id}
        type={node.type}
        data={node.data}
        positionAbsoluteX={0}
        positionAbsoluteY={0}
        dragging={false}
        selected={false}
        isConnectable
        zIndex={0}
        deletable={false}
        draggable
        selectable
      />
    </ReactFlowProvider>,
  )
}

describe('StageNode', () => {
  it('renders the stage title and id', () => {
    const { getByTestId } = renderNode(stageNode())
    const card = getByTestId('stage-node')
    expect(card).toHaveAttribute('data-phase-id', 'gather')
    expect(card.textContent).toContain('Gather')
    expect(card.textContent).toContain('gather')
  })

  it('shows the start badge only when isStart is true', () => {
    const { queryByText, rerender } = renderNode(stageNode({}, { isStart: false }))
    expect(queryByText('start')).toBeNull()

    rerender(
      <ReactFlowProvider>
        <StageNode
          id="workflow-phase:gather"
          type="stage"
          data={stageNode({}, { isStart: true }).data}
          positionAbsoluteX={0}
          positionAbsoluteY={0}
          dragging={false}
          selected={false}
          isConnectable
          zIndex={0}
          deletable={false}
          draggable
          selectable
        />
      </ReactFlowProvider>,
    )
    expect(queryByText('start')).not.toBeNull()
  })

  it('renders required-check badges without allowed-tool chips', () => {
    // Allowed-tool chips were removed once stage→tool edges took over the
    // job of showing which tools each phase admits — the edges already give
    // the user that information visually, so the badges were redundant.
    const { getByTestId } = renderNode(stageNode())
    const card = getByTestId('stage-node')
    expect(card.textContent).toContain('tool: read')
    expect(card.textContent).not.toContain('Search')
  })

  it('does not render a +N overflow indicator on the card', () => {
    const { getByTestId } = renderNode(
      stageNode({
        allowedTools: ['read', 'search', 'list', 'find', 'patch', 'edit'],
      }),
    )
    const card = getByTestId('stage-node')
    expect(card.textContent).not.toMatch(/\+\d+/)
  })
})
