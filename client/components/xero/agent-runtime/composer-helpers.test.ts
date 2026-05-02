import { describe, expect, it } from 'vitest'

import {
  buildComposerAgentSelectionKey,
  getComposerControlInput,
  parseComposerAgentSelectionKey,
  runtimeAgentIdForCustomBaseCapability,
} from '@/components/xero/agent-runtime/composer-helpers'
import type { AgentProviderModelView } from '@/src/features/xero/use-xero-desktop-state'
import type { AgentDefinitionSummaryDto } from '@/src/lib/xero-model'

const baseModel: AgentProviderModelView = {
  selectionKey: 'profile-1::gpt-omega',
  profileId: 'profile-1',
  profileLabel: 'OpenAI',
  providerId: 'openai_codex',
  providerLabel: 'OpenAI Codex',
  modelId: 'gpt-omega',
  label: 'gpt-omega',
  displayName: 'gpt-omega',
  groupId: 'openai_codex',
  groupLabel: 'OpenAI Codex',
  availability: 'available',
  availabilityLabel: 'Available',
  thinkingSupported: true,
  thinkingEffortOptions: ['minimal', 'low', 'medium', 'high'],
  defaultThinkingEffort: 'medium',
}

const customDefinition: AgentDefinitionSummaryDto = {
  definitionId: 'project_research',
  currentVersion: 2,
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

describe('runtimeAgentIdForCustomBaseCapability', () => {
  it('maps each base capability profile to the matching runtime agent id', () => {
    expect(runtimeAgentIdForCustomBaseCapability('observe_only')).toBe('ask')
    expect(runtimeAgentIdForCustomBaseCapability('engineering')).toBe('engineer')
    expect(runtimeAgentIdForCustomBaseCapability('debugging')).toBe('debug')
    expect(runtimeAgentIdForCustomBaseCapability('agent_builder')).toBe('agent_create')
  })
})

describe('buildComposerAgentSelectionKey', () => {
  it('uses a builtin: prefix for runtime agents without a custom definition', () => {
    expect(buildComposerAgentSelectionKey('engineer', null)).toBe('builtin:engineer')
  })

  it('uses a custom: prefix when an agent definition id is supplied', () => {
    expect(buildComposerAgentSelectionKey('ask', 'project_research')).toBe(
      'custom:project_research',
    )
  })

  it('treats whitespace-only definition ids as missing and falls back to the builtin', () => {
    expect(buildComposerAgentSelectionKey('debug', '   ')).toBe('builtin:debug')
  })
})

describe('parseComposerAgentSelectionKey', () => {
  it('parses builtin selection keys to their runtime agent descriptor', () => {
    const parsed = parseComposerAgentSelectionKey('builtin:agent_create', [])
    expect(parsed).toMatchObject({
      runtimeAgentId: 'agent_create',
      agentDefinitionId: null,
      isCustom: false,
    })
  })

  it('parses custom selection keys when the definition is known', () => {
    const parsed = parseComposerAgentSelectionKey(
      'custom:project_research',
      [customDefinition],
    )
    expect(parsed).toMatchObject({
      runtimeAgentId: 'ask',
      agentDefinitionId: 'project_research',
      label: 'Project Research',
      isCustom: true,
      scope: 'project_custom',
    })
  })

  it('returns null when a custom selection references an unknown definition', () => {
    const parsed = parseComposerAgentSelectionKey('custom:missing', [customDefinition])
    expect(parsed).toBeNull()
  })
})

describe('getComposerControlInput', () => {
  it('passes through the supplied agentDefinitionId on run controls', () => {
    const input = getComposerControlInput({
      runtimeAgentId: 'engineer',
      agentDefinitionId: 'team_engineer_v2',
      models: [baseModel],
      selectionKey: baseModel.selectionKey,
      thinkingEffort: 'medium',
      approvalMode: 'suggest',
    })
    expect(input).not.toBeNull()
    expect(input?.agentDefinitionId).toBe('team_engineer_v2')
    expect(input?.runtimeAgentId).toBe('engineer')
  })

  it('null-coalesces an empty agentDefinitionId so built-ins do not pin a custom version', () => {
    const input = getComposerControlInput({
      runtimeAgentId: 'ask',
      agentDefinitionId: '   ',
      models: [baseModel],
      selectionKey: baseModel.selectionKey,
      thinkingEffort: null,
      approvalMode: 'suggest',
    })
    expect(input?.agentDefinitionId).toBeNull()
  })
})
