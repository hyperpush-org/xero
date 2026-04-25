'use client'

import type { WorkflowPaneView } from '@/src/features/cadence/use-cadence-desktop-state'

interface PhaseViewProps {
  workflow?: WorkflowPaneView
  onStartRun?: () => Promise<unknown>
  onOpenSettings?: () => void
  canStartRun?: boolean
  isStartingRun?: boolean
}

export function PhaseView(_props: PhaseViewProps) {
  return null
}
