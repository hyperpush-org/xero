import type { CustomAgentWorkflowPhaseDto } from './agent-definition'
import type {
  AgentToolEffectClassDto,
  AgentToolSummaryDto,
} from './workflow-agents'

// Stage presets — one-click templates that drop a multi-stage state machine
// into the canvas instead of forcing the user to author each stage by hand.
// Presets are pure data plus a "which of the agent's wired tools are allowed
// here" filter. The empty-state chip on the canvas applies a preset by
// computing each stage's `allowedTools` against the agent's currently-wired
// tools, then writing the resulting workflowStructure into the editing detail.

export interface StagePresetStageSpec {
  id: string
  title: string
  description: string
  // Predicate run against each tool the agent currently has wired. Tools the
  // predicate selects become the stage's allowedTools list. An empty result
  // is preserved so the runtime falls back to "all tools allowed" (matching
  // the existing semantics documented in the stage editor).
  selectTool: (tool: AgentToolSummaryDto) => boolean
}

export interface StagePreset {
  id: string
  title: string
  description: string
  stages: StagePresetStageSpec[]
}

const READ_ONLY_EFFECTS = new Set<AgentToolEffectClassDto>([
  'observe',
  'runtime_state',
])

const PLAN_GROUPS = new Set<string>([
  'agent_ops',
  'agent_builder',
  'core',
  'project_context_write',
])

const EXECUTE_EFFECTS = new Set<AgentToolEffectClassDto>([
  'write',
  'destructive_write',
  'command',
  'process_control',
  'browser_control',
  'device_control',
  'external_service',
  'agent_delegation',
])

const isReadOnly = (tool: AgentToolSummaryDto) =>
  READ_ONLY_EFFECTS.has(tool.effectClass)

const isPlanning = (tool: AgentToolSummaryDto) =>
  PLAN_GROUPS.has(tool.group ?? '') || READ_ONLY_EFFECTS.has(tool.effectClass)

const isExecution = (tool: AgentToolSummaryDto) =>
  EXECUTE_EFFECTS.has(tool.effectClass)

export const STAGE_PRESETS: readonly StagePreset[] = [
  {
    id: 'research_plan_execute',
    title: 'Research → Plan → Execute',
    description:
      'Three stages: gather context with read-only tools, draft a plan, then execute writes. Forces the agent to think before it edits.',
    stages: [
      {
        id: 'research',
        title: 'Research',
        description:
          'Gather context. Only read-class tools are allowed at this stage.',
        selectTool: isReadOnly,
      },
      {
        id: 'plan',
        title: 'Plan',
        description:
          'Draft an approach using planning and project-context tools before any writes.',
        selectTool: isPlanning,
      },
      {
        id: 'execute',
        title: 'Execute',
        description:
          'Carry out the plan. Mutating tools (file writes, commands, browser control) become available here.',
        selectTool: isExecution,
      },
    ],
  },
  {
    id: 'read_only_audit',
    title: 'Read-only audit',
    description:
      'A single stage that allows only read-class tools, regardless of what else the agent has wired. Useful for scoped review or recon agents.',
    stages: [
      {
        id: 'audit',
        title: 'Audit',
        description:
          'Read-only review. The agent cannot write, run commands, or control external services from this stage.',
        selectTool: isReadOnly,
      },
    ],
  },
]

// Resolve a preset against the agent's current tool catalog. Each preset
// stage becomes a CustomAgentWorkflowPhaseDto whose allowedTools is the
// subset of the agent's wired tools the preset selects.
export function applyStagePreset(
  preset: StagePreset,
  agentTools: readonly AgentToolSummaryDto[],
): { startPhaseId: string; phases: CustomAgentWorkflowPhaseDto[] } {
  const phases: CustomAgentWorkflowPhaseDto[] = preset.stages.map(
    (stage, index, all) => {
      const allowed = agentTools
        .filter((tool) => stage.selectTool(tool))
        .map((tool) => tool.name)
      const branches: CustomAgentWorkflowPhaseDto['branches'] =
        index < all.length - 1
          ? [
              {
                targetPhaseId: all[index + 1].id,
                condition: { kind: 'always' },
              },
            ]
          : undefined
      return {
        id: stage.id,
        title: stage.title,
        description: stage.description,
        allowedTools: allowed.length > 0 ? allowed : undefined,
        requiredChecks: undefined,
        retryLimit: undefined,
        branches,
      }
    },
  )
  return {
    startPhaseId: phases[0]?.id ?? preset.stages[0]?.id ?? '',
    phases,
  }
}
