'use client'

import { createContext, useCallback, useContext, useMemo, type ReactNode } from 'react'

import type {
  AgentAuthoringCatalogDto,
  AgentToolPackCatalogDto,
} from '@/src/lib/xero-model/workflow-agents'
import type { AgentDefinitionValidationDiagnosticDto } from '@/src/lib/xero-model/agent-definition'

import type { AgentGraphNodeData, AgentInferredAdvanced } from './build-agent-graph'
import { emptyInferredAdvanced } from './build-agent-graph'

export type CanvasMode = 'create' | 'edit' | 'duplicate'

export interface CanvasModeContextValue {
  editing: boolean
  mode: CanvasMode | null
  updateNodeData: (
    nodeId: string,
    updater: (current: AgentGraphNodeData) => AgentGraphNodeData,
  ) => void
  removeNode: (nodeId: string) => void
  removeToolGroup: (sourceGroups: readonly string[]) => void
  // Pickable catalogs surfaced to node bodies. null while loading; nodes
  // should render a "loading…" affordance in that case rather than letting
  // users free-form type values.
  authoringCatalog: AgentAuthoringCatalogDto | null
  // Tool-pack manifests so the granular policy editor can render a pack
  // picker, expand pack -> tools for chip rendering, and warn when a pack
  // is denied at the same time it's allowed. null while the adapter hasn't
  // returned, or when the host doesn't expose the catalog (degrades to
  // hiding the pack picker).
  toolPackCatalog: AgentToolPackCatalogDto | null
  // Tool groups and capability flags implied by what the user has already
  // connected on the canvas. The agent header properties panel auto-checks
  // and locks these so the saved policy stays in sync with the visible
  // graph instead of relying on the user to keep them aligned.
  inferredAdvanced: AgentInferredAdvanced
  // Last preview-derived diagnostics related to the tool policy (validator
  // codes starting with `agent_definition_tool_`, `agent_definition_effect_`,
  // or `agent_definition_subagent_role_`). Empty when no preview has run
  // yet or the preview is clean. Sourced from previewAgentDefinition so we
  // never re-implement the runtime resolver in TS.
  policyDiagnostics: AgentDefinitionValidationDiagnosticDto[]
  // True while a preview refresh is in flight. The editor renders a quiet
  // spinner so the user knows diagnostics may shift.
  policyDiagnosticsLoading: boolean
  // The stages currently authored on the canvas, in the order they appear in
  // workflowStructure.phases. Used by the stage editor to populate the exits
  // target dropdown and to derive the diagnostic path index for inline
  // validation. Empty when the canvas has no stages.
  stageList: ReadonlyArray<{ id: string; title: string }>
  // Names of tools currently wired to the agent (one entry per `tool` node on
  // the canvas). Surfaced so the stage editor can constrain its allowed-tools
  // picker to tools the agent actually has, instead of the entire catalog.
  agentToolNames: ReadonlyArray<string>
}

const NOOP: CanvasModeContextValue = {
  editing: false,
  mode: null,
  updateNodeData: () => {},
  removeNode: () => {},
  removeToolGroup: () => {},
  authoringCatalog: null,
  toolPackCatalog: null,
  inferredAdvanced: emptyInferredAdvanced(),
  policyDiagnostics: [],
  policyDiagnosticsLoading: false,
  stageList: [],
  agentToolNames: [],
}

const CanvasModeContext = createContext<CanvasModeContextValue>(NOOP)

export function CanvasModeProvider({
  value,
  children,
}: {
  value: CanvasModeContextValue
  children: ReactNode
}) {
  const stable = useMemo<CanvasModeContextValue>(
    () => value,
    [
      value.editing,
      value.mode,
      value.updateNodeData,
      value.removeNode,
      value.removeToolGroup,
      value.authoringCatalog,
      value.toolPackCatalog,
      value.inferredAdvanced,
      value.policyDiagnostics,
      value.policyDiagnosticsLoading,
      value.stageList,
      value.agentToolNames,
    ],
  )
  return <CanvasModeContext.Provider value={stable}>{children}</CanvasModeContext.Provider>
}

export function useCanvasMode(): CanvasModeContextValue {
  return useContext(CanvasModeContext)
}

export function useNodeDataUpdater<T extends AgentGraphNodeData>(nodeId: string) {
  const { updateNodeData } = useCanvasMode()
  return useCallback(
    (updater: (current: T) => T) => {
      updateNodeData(nodeId, (current) => updater(current as T) as AgentGraphNodeData)
    },
    [nodeId, updateNodeData],
  )
}

export function useNodeRemover(nodeId: string) {
  const { removeNode } = useCanvasMode()
  return useCallback(() => removeNode(nodeId), [nodeId, removeNode])
}
