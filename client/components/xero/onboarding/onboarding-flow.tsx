"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { ArrowLeft, ArrowRight } from "lucide-react"
import { Button } from "@/components/ui/button"
import { StepIndicator } from "./step-indicator"
import { WelcomeStep } from "./steps/welcome-step"
import { LocalEnvironmentStep } from "./steps/local-environment-step"
import { ProvidersStep } from "./steps/providers-step"
import { ProjectStep } from "./steps/project-step"
import { NotificationsStep } from "./steps/notifications-step"
import { EnvironmentAccessStep } from "./steps/environment-access-step"
import { ConfirmationStep } from "./steps/confirmation-step"
import type {
  NotificationRouteHealthView,
  NotificationRouteMutationStatus,
  OperatorActionErrorView,
  ProviderCredentialsLoadStatus,
  ProviderCredentialsSaveStatus,
} from "@/src/features/xero/use-xero-desktop-state"
import {
  type ProviderCredentialsSnapshotDto,
  type ProviderAuthSessionView,
  type RuntimeProviderIdDto,
  type RuntimeSessionView,
  type UpsertNotificationRouteRequestDto,
  type UpsertProviderCredentialRequestDto,
} from "@/src/lib/xero-model"
import type {
  EnvironmentDiscoveryStatusDto,
  EnvironmentPermissionDecisionStatusDto,
  EnvironmentPermissionRequestDto,
} from "@/src/lib/xero-model/environment"
import { getCloudProviderPreset } from "@/src/lib/xero-model/provider-presets"
import { type OnboardingStepId } from "./types"

const BASE_STEP_ORDER: Array<{ id: OnboardingStepId; showIndicator: boolean }> = [
  { id: "welcome", showIndicator: false },
  { id: "providers", showIndicator: true },
  { id: "project", showIndicator: true },
  { id: "notifications", showIndicator: true },
  { id: "confirm", showIndicator: true },
]

/**
 * Pure helper exported for unit tests. Computes the active step order given
 * the current launch mode and whether environment-access decisions are
 * required.
 */
export function computeStepOrder(
  launchMode: string | null | undefined,
  hasEnvironmentPermissionRequests: boolean,
): Array<{ id: OnboardingStepId; showIndicator: boolean }> {
  const withLocalEnv =
    launchMode === "local-source"
      ? BASE_STEP_ORDER.flatMap((step) =>
          step.id === "providers"
            ? [{ id: "local-environment" as const, showIndicator: true }, step]
            : [step],
        )
      : BASE_STEP_ORDER

  if (!hasEnvironmentPermissionRequests) {
    return withLocalEnv
  }

  return withLocalEnv.flatMap((step) =>
    step.id === "confirm"
      ? [{ id: "environment-access" as const, showIndicator: true }, step]
      : [step],
  )
}

interface ImportedProjectView {
  name: string
  path: string
}

function getProviderReview(
  providerCredentials: ProviderCredentialsSnapshotDto | null,
  runtimeSession: RuntimeSessionView | null,
) {
  const credentials = providerCredentials?.credentials ?? []
  if (credentials.length === 0) {
    return {
      ready: false,
      value: 'No provider set up yet',
    }
  }

  // Prefer a connected OAuth provider; fall back to first credentialed provider.
  const connectedOAuth =
    runtimeSession?.isAuthenticated && runtimeSession.providerId === 'openai_codex'
      ? credentials.find((c) => c.providerId === 'openai_codex') ?? null
      : null
  const review = connectedOAuth ?? credentials[0]
  const preset = getCloudProviderPreset(review.providerId)
  const label = preset?.label ?? review.providerId
  const additionalSuffix = credentials.length > 1 ? ` · ${credentials.length - 1} more configured` : ''

  switch (review.kind) {
    case 'api_key':
      return {
        ready: true,
        value: `${label} · API key saved${additionalSuffix}`,
      }
    case 'oauth_session':
      return {
        ready: true,
        value: `${label} · signed in${additionalSuffix}`,
      }
    case 'local':
      return {
        ready: true,
        value: `${label} · local endpoint ready${additionalSuffix}`,
      }
    case 'ambient':
      return {
        ready: true,
        value: `${label} · ambient auth ready${additionalSuffix}`,
      }
  }
}

export interface OnboardingFlowProps {
  providerCredentials: ProviderCredentialsSnapshotDto | null
  providerCredentialsLoadStatus: ProviderCredentialsLoadStatus
  providerCredentialsLoadError: OperatorActionErrorView | null
  providerCredentialsSaveStatus: ProviderCredentialsSaveStatus
  providerCredentialsSaveError: OperatorActionErrorView | null
  runtimeSession: RuntimeSessionView | null
  project: ImportedProjectView | null
  isImporting: boolean
  isProjectLoading: boolean
  projectErrorMessage: string | null
  notificationRoutes: NotificationRouteHealthView[]
  notificationRouteMutationStatus: NotificationRouteMutationStatus
  pendingNotificationRouteId: string | null
  notificationRouteMutationError: OperatorActionErrorView | null
  environmentPermissionRequests?: EnvironmentPermissionRequestDto[]
  onResolveEnvironmentPermissions?: (decisions: Array<{
    id: string
    status: EnvironmentPermissionDecisionStatusDto
  }>) => Promise<EnvironmentDiscoveryStatusDto | null>
  /**
   * "local-source" when the app was launched via `pnpm start` from a source
   * checkout; null otherwise. Toggles the local-environment onboarding step.
   */
  launchMode?: string | null
  onImportProject: () => Promise<void>
  onRefreshProviderCredentials?: (options?: {
    force?: boolean
  }) => Promise<ProviderCredentialsSnapshotDto>
  onUpsertProviderCredential: (
    request: UpsertProviderCredentialRequestDto,
  ) => Promise<ProviderCredentialsSnapshotDto>
  onDeleteProviderCredential?: (
    providerId: RuntimeProviderIdDto,
  ) => Promise<ProviderCredentialsSnapshotDto>
  onStartOAuthLogin?: (request: {
    providerId: RuntimeProviderIdDto
    originator?: string | null
  }) => Promise<ProviderAuthSessionView | null>
  onUpsertNotificationRoute: (
    request: Omit<UpsertNotificationRouteRequestDto, "projectId">,
  ) => Promise<unknown>
  onComplete: () => void
  onDismiss: () => void
}

export function OnboardingFlow({
  providerCredentials,
  providerCredentialsLoadStatus,
  providerCredentialsLoadError,
  providerCredentialsSaveStatus,
  providerCredentialsSaveError,
  runtimeSession,
  project,
  isImporting,
  isProjectLoading,
  projectErrorMessage,
  notificationRoutes,
  notificationRouteMutationStatus,
  pendingNotificationRouteId,
  notificationRouteMutationError,
  environmentPermissionRequests = [],
  onResolveEnvironmentPermissions,
  launchMode = null,
  onImportProject,
  onRefreshProviderCredentials,
  onUpsertProviderCredential,
  onDeleteProviderCredential,
  onStartOAuthLogin,
  onUpsertNotificationRoute,
  onComplete,
  onDismiss,
}: OnboardingFlowProps) {
  const [stepIndex, setStepIndex] = useState(0)
  const [environmentPermissionDecisions, setEnvironmentPermissionDecisions] = useState<
    Record<string, EnvironmentPermissionDecisionStatusDto | "pending">
  >({})
  const [environmentPermissionSaveStatus, setEnvironmentPermissionSaveStatus] = useState<
    "idle" | "saving" | "error"
  >("idle")
  const [environmentPermissionSaveError, setEnvironmentPermissionSaveError] = useState<string | null>(null)
  const directionRef = useRef<1 | -1>(1)

  const stepOrder = useMemo(
    () => computeStepOrder(launchMode, environmentPermissionRequests.length > 0),
    [environmentPermissionRequests.length, launchMode],
  )
  const indicatorSteps = useMemo(
    () => stepOrder.filter((step) => step.showIndicator),
    [stepOrder],
  )
  const currentStep = stepOrder[Math.min(stepIndex, stepOrder.length - 1)]
  const providerReview = getProviderReview(providerCredentials, runtimeSession)
  const isEnvironmentAccess = currentStep.id === "environment-access"

  useEffect(() => {
    setEnvironmentPermissionDecisions((current) => {
      const next: Record<string, EnvironmentPermissionDecisionStatusDto | "pending"> = {}
      for (const request of environmentPermissionRequests) {
        if (request.status === "granted" || request.status === "denied" || request.status === "skipped") {
          next[request.id] = request.status
        } else if (current[request.id]) {
          next[request.id] = current[request.id]
        } else {
          next[request.id] = request.optional ? "skipped" : "pending"
        }
      }
      return next
    })
  }, [environmentPermissionRequests])

  const goTo = useCallback((target: number) => {
    setStepIndex((current) => {
      const clamped = Math.max(0, Math.min(stepOrder.length - 1, target))
      directionRef.current = clamped >= current ? 1 : -1
      return clamped
    })
  }, [stepOrder.length])

  const next = useCallback(() => goTo(stepIndex + 1), [goTo, stepIndex])
  const back = useCallback(() => goTo(stepIndex - 1), [goTo, stepIndex])
  const setEnvironmentPermissionDecision = useCallback(
    (requestId: string, status: EnvironmentPermissionDecisionStatusDto) => {
      setEnvironmentPermissionSaveStatus("idle")
      setEnvironmentPermissionSaveError(null)
      setEnvironmentPermissionDecisions((current) => ({
        ...current,
        [requestId]: status,
      }))
    },
    [],
  )

  const hasRequiredEnvironmentPermissionPending = useMemo(
    () =>
      environmentPermissionRequests.some(
        (request) =>
          !request.optional && environmentPermissionDecisions[request.id] !== "granted",
      ),
    [environmentPermissionDecisions, environmentPermissionRequests],
  )

  const saveEnvironmentAccessAndContinue = useCallback(async () => {
    if (hasRequiredEnvironmentPermissionPending) {
      return
    }

    const decisions = environmentPermissionRequests
      .map((request) => ({
        id: request.id,
        status:
          environmentPermissionDecisions[request.id] ??
          (request.optional ? "skipped" : "granted"),
      }))
      .filter(
        (decision): decision is {
          id: string
          status: EnvironmentPermissionDecisionStatusDto
        } => decision.status !== "pending",
      )

    if (decisions.length > 0 && onResolveEnvironmentPermissions) {
      setEnvironmentPermissionSaveStatus("saving")
      try {
        await onResolveEnvironmentPermissions(decisions)
        setEnvironmentPermissionSaveStatus("idle")
        setEnvironmentPermissionSaveError(null)
      } catch {
        setEnvironmentPermissionSaveStatus("error")
        setEnvironmentPermissionSaveError("Xero could not save environment access choices. Try again.")
        return
      }
    }

    next()
  }, [
    environmentPermissionDecisions,
    environmentPermissionRequests,
    hasRequiredEnvironmentPermissionPending,
    next,
    onResolveEnvironmentPermissions,
  ])

  const indicatorIndex = useMemo(
    () => Math.max(0, indicatorSteps.findIndex((step) => step.id === currentStep.id)),
    [currentStep.id, indicatorSteps],
  )

  const showFooter = currentStep.id !== "welcome"
  const isConfirm = currentStep.id === "confirm"
  const primaryLabel = isConfirm
    ? "Enter Xero"
    : isEnvironmentAccess && environmentPermissionSaveStatus === "saving"
      ? "Saving"
      : "Continue"
  const handlePrimary = isConfirm
    ? onComplete
    : isEnvironmentAccess
      ? () => void saveEnvironmentAccessAndContinue()
      : next
  const primaryDisabled =
    (isEnvironmentAccess && hasRequiredEnvironmentPermissionPending) ||
    environmentPermissionSaveStatus === "saving"

  return (
    <div className="relative flex min-h-full flex-1 flex-col overflow-hidden bg-background text-foreground">
      <header className="relative z-10 flex shrink-0 items-center justify-between gap-3 px-8 pt-5">
        <div className="min-w-[72px]">
          {currentStep.showIndicator ? (
            <StepIndicator total={indicatorSteps.length} currentIndex={indicatorIndex} />
          ) : null}
        </div>

        <Button
          variant="ghost"
          size="sm"
          onClick={onDismiss}
          className="h-7 text-[12px] text-muted-foreground hover:text-foreground"
        >
          Skip setup
        </Button>
      </header>

      <main className="relative z-10 flex-1 overflow-y-auto">
        <div className="flex min-h-full items-center justify-center px-8 py-10">
          <div
            key={currentStep.id}
            className={`w-full ${currentStep.id === "providers" ? "max-w-xl" : "max-w-md"} animate-in fade-in-0 motion-enter ${
              directionRef.current === 1 ? "slide-in-from-right-4" : "slide-in-from-left-4"
            }`}
          >
            {currentStep.id === "welcome" ? (
            <WelcomeStep onContinue={next} onSkipAll={onDismiss} />
          ) : null}
          {currentStep.id === "local-environment" ? (
            <LocalEnvironmentStep />
          ) : null}
          {currentStep.id === "providers" ? (
            <ProvidersStep
              providerCredentials={providerCredentials}
              providerCredentialsLoadStatus={providerCredentialsLoadStatus}
              providerCredentialsLoadError={providerCredentialsLoadError}
              providerCredentialsSaveStatus={providerCredentialsSaveStatus}
              providerCredentialsSaveError={providerCredentialsSaveError}
              runtimeSession={runtimeSession}
              onRefreshProviderCredentials={onRefreshProviderCredentials}
              onUpsertProviderCredential={onUpsertProviderCredential}
              onDeleteProviderCredential={onDeleteProviderCredential}
              onStartOAuthLogin={onStartOAuthLogin}
            />
          ) : null}
          {currentStep.id === "project" ? (
            <ProjectStep
              project={project}
              isImporting={isImporting}
              isProjectLoading={isProjectLoading}
              errorMessage={projectErrorMessage}
              onImportProject={() => void onImportProject()}
            />
          ) : null}
          {currentStep.id === "notifications" ? (
            <NotificationsStep
              projectName={project?.name ?? null}
              routes={notificationRoutes}
              mutationStatus={notificationRouteMutationStatus}
              pendingRouteId={pendingNotificationRouteId}
              mutationError={notificationRouteMutationError}
              onUpsertNotificationRoute={onUpsertNotificationRoute}
            />
          ) : null}
          {currentStep.id === "environment-access" ? (
            <EnvironmentAccessStep
              permissionRequests={environmentPermissionRequests}
              decisions={environmentPermissionDecisions}
              disabled={environmentPermissionSaveStatus === "saving"}
              onDecisionChange={setEnvironmentPermissionDecision}
            />
          ) : null}
          {currentStep.id === "environment-access" && environmentPermissionSaveError ? (
            <p className="mt-3 text-[11.5px] leading-relaxed text-destructive">
              {environmentPermissionSaveError}
            </p>
          ) : null}
          {currentStep.id === "confirm" ? (
            <ConfirmationStep
              providerValue={providerReview.value}
              providerReady={providerReview.ready}
              projectName={project?.name ?? null}
              notifications={notificationRoutes}
            />
          ) : null}
          </div>
        </div>
      </main>

      {showFooter ? (
        <footer className="relative z-10 shrink-0">
          <div
            className={`mx-auto flex w-full ${
              currentStep.id === "providers" ? "max-w-xl" : "max-w-md"
            } items-center justify-between gap-2 px-8 pb-6`}
          >
            <Button
              variant="ghost"
              size="sm"
              onClick={back}
              disabled={stepIndex <= 1}
              className="h-8 gap-1.5 px-2 text-[12px] text-muted-foreground hover:text-foreground"
            >
              <ArrowLeft className="h-3.5 w-3.5" />
              Back
            </Button>

            <div className="flex items-center gap-1">
              {!isConfirm ? (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={
                    isEnvironmentAccess
                      ? () => void saveEnvironmentAccessAndContinue()
                      : next
                  }
                  disabled={
                    (isEnvironmentAccess && hasRequiredEnvironmentPermissionPending) ||
                    environmentPermissionSaveStatus === "saving"
                  }
                  className="h-8 text-[12px] text-muted-foreground hover:text-foreground"
                >
                  Skip
                </Button>
              ) : null}
              <Button
                size="sm"
                onClick={handlePrimary}
                disabled={primaryDisabled}
                className="group h-8 gap-1.5 bg-primary px-3 text-[12px] font-medium hover:bg-primary/90"
              >
                {primaryLabel}
                <ArrowRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-0.5" />
              </Button>
            </div>
          </div>
        </footer>
      ) : null}
    </div>
  )
}
