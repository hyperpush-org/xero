import { describe, expect, it } from 'vitest'

import {
  agentDefinitionSummarySchema,
  getAgentDefinitionBaseCapabilityLabel,
} from './agent-definition'

describe('agent definition contracts', () => {
  it('accepts the built-in harness test profile in registry summaries', () => {
    const summary = agentDefinitionSummarySchema.parse({
      definitionId: 'test',
      currentVersion: 1,
      displayName: 'Test',
      shortLabel: 'Test',
      description:
        'Run the dev harness through the normal owned-agent conversation, provider, tool, stream, and persistence path.',
      scope: 'built_in',
      lifecycleState: 'active',
      baseCapabilityProfile: 'harness_test',
      createdAt: '2026-05-01T00:00:00Z',
      updatedAt: '2026-05-01T00:00:00Z',
      isBuiltIn: true,
    })

    expect(summary.baseCapabilityProfile).toBe('harness_test')
    expect(getAgentDefinitionBaseCapabilityLabel('harness_test')).toBe('Harness Test')
  })
})
