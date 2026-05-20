import { startTransition, useCallback } from 'react'

import { mapAutonomousRunInspection } from '@/src/lib/xero-model/autonomous'
import {
  mapAgentSession,
  mapRuntimeRun,
  selectAgentSessionId,
  type RuntimeAutoCompactPreferenceDto,
  type RuntimeRunControlInputDto,
  type RuntimeRunView,
  type StagedAgentAttachmentDto,
  type UpdateRuntimeRunControlsRequestDto,
} from '@/src/lib/xero-model/runtime'

import type {
  XeroDesktopMutationActions,
  UseXeroDesktopMutationsArgs,
} from './mutation-support'
import {
  getActiveProjectId,
  getOperatorActionError,
} from './mutation-support'

const MAX_FALLBACK_SESSION_TITLE_CHARS = 64
const GENERIC_SESSION_TITLES = new Set([
  'main',
  'new chat',
  'new session',
  'untitled',
  'untitled session',
  'chat',
  'session',
  'conversation',
  'developer conversation',
  'developer assistant conversation',
])

function hasPromptPayload(value: string | null | undefined): boolean {
  return typeof value === 'string' && value.trim().length > 0
}

function hasAttachmentPayload(value: StagedAgentAttachmentDto[] | null | undefined): boolean {
  return Array.isArray(value) && value.length > 0
}

function scheduleRuntimeRunProjectionUpdate(callback: () => void) {
  if (typeof window === 'undefined') {
    startTransition(callback)
    return
  }

  window.setTimeout(() => {
    startTransition(callback)
  }, 16)
}

function collapseSessionTitleWhitespace(value: string): string {
  return value.split(/\s+/u).filter(Boolean).join(' ')
}

function trimTrailingSessionTitlePunctuation(value: string): string {
  return value
    .trim()
    .replace(/[.,:;!?\-_"'`]+$/u, '')
    .trim()
}

function truncateSessionTitle(value: string, maxChars: number): string {
  const trimmed = value.trim()
  const chars = Array.from(trimmed)
  if (chars.length <= maxChars) {
    return trimmed
  }

  let output = ''
  for (const word of trimmed.split(/\s+/u)) {
    const nextLength = Array.from(output).length + (output.length === 0 ? 0 : 1) + Array.from(word).length
    if (nextLength > maxChars) {
      break
    }
    output = output.length === 0 ? word : `${output} ${word}`
  }

  return output.length > 0 ? output : chars.slice(0, maxChars).join('')
}

function isGenericSessionTitle(value: string): boolean {
  const normalized = collapseSessionTitleWhitespace(value.trim().replace(/^["'`]+|["'`]+$/gu, '').toLowerCase())
  return GENERIC_SESSION_TITLES.has(normalized)
}

function fallbackSessionTitleFromPrompt(prompt: string): string | null {
  const firstLine = prompt
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .find((line) => line.length > 0) ?? ''
  const cleaned = collapseSessionTitleWhitespace(firstLine)
    .replace(/^[#\-*>"'`]+/u, '')
    .trim()
  const title = truncateSessionTitle(
    trimTrailingSessionTitlePunctuation(cleaned),
    MAX_FALLBACK_SESSION_TITLE_CHARS,
  )

  return title.length > 0 && !isGenericSessionTitle(title) ? title : null
}

function controlsForSessionTitleRefresh(
  runtimeRun: RuntimeRunView | null,
  requestedControls: RuntimeRunControlInputDto | null | undefined,
): RuntimeRunControlInputDto | null {
  if (requestedControls) {
    return requestedControls
  }
  const selected = runtimeRun?.controls.selected
  if (!selected) {
    return null
  }
  return {
    runtimeAgentId: selected.runtimeAgentId,
    agentDefinitionId: selected.agentDefinitionId,
    providerProfileId: selected.providerProfileId,
    modelId: selected.modelId,
    thinkingEffort: selected.thinkingEffort,
    approvalMode: selected.approvalMode,
    planModeRequired: selected.planModeRequired,
    autoCompactEnabled: selected.autoCompactEnabled,
  }
}

export function useRunControlMutations({
  adapter,
  refs,
  setters,
  operations,
}: UseXeroDesktopMutationsArgs): Pick<
  XeroDesktopMutationActions,
  | 'startAutonomousRun'
  | 'inspectAutonomousRun'
  | 'cancelAutonomousRun'
  | 'startRuntimeRun'
  | 'updateRuntimeRunControls'
  | 'stopRuntimeRun'
> {
  const { activeProjectIdRef, activeProjectRef, runtimeRunsRef } = refs
  const {
    setAutonomousRunActionStatus,
    setPendingAutonomousRunAction,
    setAutonomousRunActionError,
    setRuntimeRunActionStatus,
    setPendingRuntimeRunAction,
    setRuntimeRunActionError,
  } = setters
  const {
    applyAgentSessionSelection,
    syncRuntimeRun,
    syncAutonomousRun,
    applyRuntimeRunUpdate,
    applyAutonomousRunStateUpdate,
  } = operations

  const refreshSessionTitleFromPrompt = useCallback(
    (
      projectId: string,
      agentSessionId: string,
      prompt: string | null | undefined,
      controls: RuntimeRunControlInputDto | null | undefined,
    ) => {
      const promptForAutoName = prompt?.trim() ?? ''
      if (promptForAutoName.length === 0) {
        return
      }
      const applySessionTitle = (session: Parameters<typeof mapAgentSession>[0]) => {
        if (activeProjectIdRef.current === projectId) {
          scheduleRuntimeRunProjectionUpdate(() => {
            applyAgentSessionSelection(mapAgentSession(session))
          })
        }
      }

      void adapter
        .autoNameAgentSession({
          projectId,
          agentSessionId,
          prompt: promptForAutoName,
          controls: controls ?? null,
        })
        .then(applySessionTitle)
        .catch(() => {
          const fallbackTitle = fallbackSessionTitleFromPrompt(promptForAutoName)
          if (!fallbackTitle) {
            return
          }

          void adapter
            .updateAgentSession({
              projectId,
              agentSessionId,
              title: fallbackTitle,
            })
            .then(applySessionTitle)
            .catch(() => {
              // Auto-naming should never interrupt a user prompt.
            })
        })
    },
    [activeProjectIdRef, adapter, applyAgentSessionSelection],
  )

  const startAutonomousRun = useCallback(async () => {
    const projectId = getActiveProjectId(
      activeProjectIdRef,
      'Select an imported project before starting an autonomous run.',
    )
    const agentSessionId = selectAgentSessionId(
      activeProjectRef.current?.id === projectId ? activeProjectRef.current.agentSessions : null,
    )

    setAutonomousRunActionStatus('running')
    setPendingAutonomousRunAction('start')
    setAutonomousRunActionError(null)

    try {
      const response = await adapter.startAutonomousRun(projectId, agentSessionId)
      return applyAutonomousRunStateUpdate(projectId, mapAutonomousRunInspection(response), {
        clearGlobalError: false,
        loadError: null,
      })
    } catch (error) {
      setAutonomousRunActionError(
        getOperatorActionError(
          error,
          'Xero could not start or inspect the autonomous run for this project.',
        ),
      )

      try {
        await syncAutonomousRun(projectId)
      } catch {
        // Ignore follow-up refresh failures and preserve the last truthful state.
      }

      throw error
    } finally {
      setAutonomousRunActionStatus('idle')
      setPendingAutonomousRunAction(null)
    }
  }, [
    activeProjectIdRef,
    activeProjectRef,
    adapter,
    applyAutonomousRunStateUpdate,
    setAutonomousRunActionError,
    setAutonomousRunActionStatus,
    setPendingAutonomousRunAction,
    syncAutonomousRun,
  ])

  const inspectAutonomousRun = useCallback(async () => {
    const projectId = getActiveProjectId(
      activeProjectIdRef,
      'Select an imported project before inspecting autonomous run truth.',
    )

    setAutonomousRunActionStatus('running')
    setPendingAutonomousRunAction('inspect')
    setAutonomousRunActionError(null)

    try {
      return await syncAutonomousRun(projectId)
    } catch (error) {
      setAutonomousRunActionError(
        getOperatorActionError(
          error,
          'Xero could not inspect the autonomous run truth for this project.',
        ),
      )
      throw error
    } finally {
      setAutonomousRunActionStatus('idle')
      setPendingAutonomousRunAction(null)
    }
  }, [
    activeProjectIdRef,
    setAutonomousRunActionError,
    setAutonomousRunActionStatus,
    setPendingAutonomousRunAction,
    syncAutonomousRun,
  ])

  const cancelAutonomousRun = useCallback(
    async (runId: string) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before cancelling the autonomous run.',
      )
      const agentSessionId = selectAgentSessionId(
        activeProjectRef.current?.id === projectId ? activeProjectRef.current.agentSessions : null,
      )

      setAutonomousRunActionStatus('running')
      setPendingAutonomousRunAction('cancel')
      setAutonomousRunActionError(null)

      try {
        const response = await adapter.cancelAutonomousRun(projectId, agentSessionId, runId)
        return applyAutonomousRunStateUpdate(projectId, mapAutonomousRunInspection(response), {
          clearGlobalError: false,
          loadError: null,
        })
      } catch (error) {
        setAutonomousRunActionError(
          getOperatorActionError(error, 'Xero could not cancel the autonomous run for this project.'),
        )

        try {
          await syncAutonomousRun(projectId)
        } catch {
          // Ignore follow-up refresh failures and preserve the last truthful state.
        }

        throw error
      } finally {
        setAutonomousRunActionStatus('idle')
        setPendingAutonomousRunAction(null)
      }
    },
    [
      activeProjectIdRef,
      activeProjectRef,
      adapter,
      applyAutonomousRunStateUpdate,
      setAutonomousRunActionError,
      setAutonomousRunActionStatus,
      setPendingAutonomousRunAction,
      syncAutonomousRun,
    ],
  )

  const startRuntimeRun = useCallback(async (options?: { controls?: RuntimeRunControlInputDto | null; prompt?: string | null; attachments?: StagedAgentAttachmentDto[] }) => {
    const projectId = getActiveProjectId(
      activeProjectIdRef,
      'Select an imported project before starting a Xero-owned agent run.',
    )
    const agentSessionId = selectAgentSessionId(
      activeProjectRef.current?.id === projectId ? activeProjectRef.current.agentSessions : null,
    )
    const isPromptSubmission = hasPromptPayload(options?.prompt) || hasAttachmentPayload(options?.attachments)

    if (!isPromptSubmission) {
      setRuntimeRunActionStatus('running')
      setPendingRuntimeRunAction('start')
      setRuntimeRunActionError(null)
    }

    try {
      const response = await adapter.startRuntimeRun(projectId, agentSessionId, {
        initialControls: options?.controls ?? null,
        initialPrompt: options?.prompt ?? null,
        initialAttachments: options?.attachments ?? [],
      })
      const runtimeRun = mapRuntimeRun(response)
      scheduleRuntimeRunProjectionUpdate(() => {
        setRuntimeRunActionError(null)
        applyRuntimeRunUpdate(projectId, runtimeRun, {
          clearGlobalError: false,
          loadError: null,
        })
      })

      refreshSessionTitleFromPrompt(
        projectId,
        agentSessionId,
        options?.prompt,
        controlsForSessionTitleRefresh(runtimeRun, options?.controls),
      )

      return runtimeRun
    } catch (error) {
      setRuntimeRunActionError(
        getOperatorActionError(
          error,
          'Xero could not start or reconnect the Xero-owned agent run for this project.',
        ),
      )

      try {
        await syncRuntimeRun(projectId)
      } catch {
        // Ignore follow-up refresh failures and preserve the last truthful state.
      }

      throw error
    } finally {
      if (!isPromptSubmission) {
        setRuntimeRunActionStatus('idle')
        setPendingRuntimeRunAction(null)
      }
    }
  }, [
    activeProjectIdRef,
    activeProjectRef,
    adapter,
    applyRuntimeRunUpdate,
    refreshSessionTitleFromPrompt,
    setPendingRuntimeRunAction,
    setRuntimeRunActionError,
    setRuntimeRunActionStatus,
    syncRuntimeRun,
  ])

  const updateRuntimeRunControls = useCallback(
    async (request: {
      controls?: RuntimeRunControlInputDto | null
      prompt?: string | null
      attachments?: StagedAgentAttachmentDto[]
      autoCompact?: RuntimeAutoCompactPreferenceDto | null
    } = {}) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before queueing agent-run controls.',
      )
      const agentSessionId = selectAgentSessionId(
        activeProjectRef.current?.id === projectId ? activeProjectRef.current.agentSessions : null,
      )
      let runId: string | null =
        runtimeRunsRef.current[projectId]?.runId?.trim() ??
        activeProjectRef.current?.runtimeRun?.runId?.trim() ??
        null

      if (!runId) {
        try {
          const hydratedRun = await syncRuntimeRun(projectId)
          runId = hydratedRun?.runId?.trim() ?? null
        } catch {
          // Ignore refresh failure here; the queue attempt should still fail with the explicit missing-run copy below.
        }
      }

      if (!runId) {
        throw new Error('Xero cannot queue runtime-run controls until a Xero-owned agent run exists for this project.')
      }
      const resolvedRunId = runId
      const isPromptSubmission = hasPromptPayload(request.prompt) || hasAttachmentPayload(request.attachments)

      if (!isPromptSubmission) {
        setRuntimeRunActionStatus('running')
        setPendingRuntimeRunAction('update_controls')
        setRuntimeRunActionError(null)
      }

      try {
        const updateRequest: UpdateRuntimeRunControlsRequestDto = {
          projectId,
          agentSessionId,
          runId: resolvedRunId,
          controls: request.controls ?? null,
          prompt: request.prompt ?? null,
          attachments: request.attachments ?? [],
        }
        const response = await adapter.updateRuntimeRunControls(updateRequest)
        const runtimeRun = mapRuntimeRun(response)
        scheduleRuntimeRunProjectionUpdate(() => {
          setRuntimeRunActionError(null)
          applyRuntimeRunUpdate(projectId, runtimeRun, {
            clearGlobalError: false,
            loadError: null,
          })
        })
        refreshSessionTitleFromPrompt(
          projectId,
          agentSessionId,
          request.prompt,
          controlsForSessionTitleRefresh(runtimeRun, request.controls),
        )
        return runtimeRun
      } catch (error) {
        setRuntimeRunActionError(
          getOperatorActionError(
            error,
            'Xero could not queue runtime-run control changes for this project.',
          ),
        )

        try {
          await syncRuntimeRun(projectId)
        } catch {
          // Ignore follow-up refresh failures and preserve the last truthful state.
        }

        throw error
      } finally {
        if (!isPromptSubmission) {
          setRuntimeRunActionStatus('idle')
          setPendingRuntimeRunAction(null)
        }
      }
    },
    [
      activeProjectIdRef,
      activeProjectRef,
      runtimeRunsRef,
      adapter,
      applyRuntimeRunUpdate,
      refreshSessionTitleFromPrompt,
      setPendingRuntimeRunAction,
      setRuntimeRunActionError,
      setRuntimeRunActionStatus,
      syncRuntimeRun,
    ],
  )

  const stopRuntimeRun = useCallback(
    async (runId: string) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before stopping the Xero-owned agent run.',
      )
      const agentSessionId = selectAgentSessionId(
        activeProjectRef.current?.id === projectId ? activeProjectRef.current.agentSessions : null,
      )

      setRuntimeRunActionStatus('running')
      setPendingRuntimeRunAction('stop')
      setRuntimeRunActionError(null)

      try {
        const response = await adapter.stopRuntimeRun(projectId, agentSessionId, runId)
        const runtimeRun = response ? mapRuntimeRun(response) : null
        startTransition(() => {
          applyRuntimeRunUpdate(projectId, runtimeRun, {
            clearGlobalError: false,
            loadError: null,
          })
        })
        return runtimeRun
      } catch (error) {
        setRuntimeRunActionError(
          getOperatorActionError(error, 'Xero could not stop the Xero-owned agent run for this project.'),
        )

        try {
          await syncRuntimeRun(projectId)
        } catch {
          // Ignore follow-up refresh failures and preserve the last truthful state.
        }

        throw error
      } finally {
        setRuntimeRunActionStatus('idle')
        setPendingRuntimeRunAction(null)
      }
    },
    [
      activeProjectIdRef,
      activeProjectRef,
      adapter,
      applyRuntimeRunUpdate,
      setPendingRuntimeRunAction,
      setRuntimeRunActionError,
      setRuntimeRunActionStatus,
      syncRuntimeRun,
    ],
  )

  return {
    startAutonomousRun,
    inspectAutonomousRun,
    cancelAutonomousRun,
    startRuntimeRun,
    updateRuntimeRunControls,
    stopRuntimeRun,
  }
}
