import { render } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { NodePropertiesPanel } from './node-properties-panel'
import type { AgentGraphNode } from './build-agent-graph'

describe('NodePropertiesPanel', () => {
  it('uses normalized canvas spacing, content height, and the compact panel width', () => {
    const promptNode: AgentGraphNode = {
      id: 'prompt:test',
      type: 'prompt',
      position: { x: 0, y: 0 },
      data: {
        prompt: {
          id: 'test',
          label: 'Test prompt',
          role: 'system',
          source: 'custom',
          body: 'Be useful.',
        },
      },
    } as AgentGraphNode

    const { container } = render(
      <NodePropertiesPanel selectedNode={promptNode} onClose={vi.fn()} />,
    )
    const panel = container.querySelector('.agent-properties-panel')

    expect(panel).not.toBeNull()
    expect(panel).toHaveClass('left-8')
    expect(panel).toHaveClass('top-14')
    expect(panel).toHaveClass('max-h-[calc(100%-4.5rem)]')
    expect(panel).toHaveClass('w-[272px]')
    expect(panel).toHaveClass('text-[10.5px]')
  })
})
