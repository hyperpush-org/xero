'use client'

import { createContext, useContext } from 'react'

/**
 * Shared callback used by expandable card nodes to inform the canvas of
 * expansion state changes. The canvas re-runs the layout with an enlarged
 * height for expanded nodes so neighbours visibly shift to make room.
 */
export interface AgentCanvasExpansionContextValue {
  setExpanded: (nodeId: string, expanded: boolean) => void
}

export const AgentCanvasExpansionContext =
  createContext<AgentCanvasExpansionContextValue>({
    setExpanded: () => {},
  })

export function useAgentCanvasExpansion(): AgentCanvasExpansionContextValue {
  return useContext(AgentCanvasExpansionContext)
}
