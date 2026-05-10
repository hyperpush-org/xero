'use client'

import { memo } from 'react'
import type { NodeProps } from '@xyflow/react'

import type { LaneLabelFlowNode } from '../build-agent-graph'

// Lane label tone keys map to lane ids emitted by `layoutAgentGraphByCategory`.
// The colour bar prefix matches the edge family for that lane so the user can
// link the lane to its edges at a glance.
const LANE_TONE_BY_ID: Record<string, string> = {
  'lane:prompt': 'agent-graph-lane-label--prompt',
  'lane:skills': 'agent-graph-lane-label--skills',
  'lane:tool': 'agent-graph-lane-label--tool',
  'lane:db-table': 'agent-graph-lane-label--db',
  'lane:agent-output': 'agent-graph-lane-label--output',
  'lane:output-section': 'agent-graph-lane-label--output',
  'lane:consumed-artifact': 'agent-graph-lane-label--consume',
}

export const LaneLabelNode = memo(function LaneLabelNode({
  id,
  data,
  width,
}: NodeProps<LaneLabelFlowNode>) {
  const toneClass = LANE_TONE_BY_ID[id] ?? ''

  return (
    <div
      className={`agent-graph-lane-label ${toneClass}`}
      style={typeof width === 'number' ? { width } : undefined}
    >
      <span aria-hidden="true" className="agent-graph-lane-label__bar" />
      <span className="agent-graph-lane-label__text">{data.label}</span>
      {data.count > 0 ? (
        <span className="agent-graph-lane-label__count">{data.count}</span>
      ) : null}
    </div>
  )
})
