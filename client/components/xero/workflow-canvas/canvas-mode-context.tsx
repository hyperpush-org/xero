'use client'

import { createContext, useCallback, useContext, useMemo, type ReactNode } from 'react'

import type { AgentAuthoringCatalogDto } from '@/src/lib/xero-model/workflow-agents'

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
  // Tool groups and capability flags implied by what the user has already
  // connected on the canvas. The agent header properties panel auto-checks
  // and locks these so the saved policy stays in sync with the visible
  // graph instead of relying on the user to keep them aligned.
  inferredAdvanced: AgentInferredAdvanced
}

const NOOP: CanvasModeContextValue = {
  editing: false,
  mode: null,
  updateNodeData: () => {},
  removeNode: () => {},
  removeToolGroup: () => {},
  authoringCatalog: null,
  inferredAdvanced: emptyInferredAdvanced(),
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
      value.inferredAdvanced,
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
