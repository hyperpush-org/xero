"use client"

import { useCallback, useMemo, useRef, useState } from "react"
import { ArrowLeft, ArrowRight } from "lucide-react"
import { Button } from "@/components/ui/button"
import { StepIndicator } from "./step-indicator"
import { WelcomeStep } from "./steps/welcome-step"
import { ProvidersStep } from "./steps/providers-step"
import { ProjectStep } from "./steps/project-step"
import { NotificationsStep } from "./steps/notifications-step"
import { ConfirmationStep } from "./steps/confirmation-step"
import type {
  NotificationRouteHealthView,
  NotificationRouteMutationStatus,
  OperatorActionErrorView,
  ProviderCredentialsLoadStatus,
  ProviderCredentialsSaveStatus,
  ProviderModelCatalogLoadStatus,
} from "@/src/features/cadence/use-cadence-desktop-state"
import {
  type ProviderCredentialsSnapshotDto,
  type ProviderAuthSessionView,
  type ProviderModelCatalogDto,
  type RuntimeProviderIdDto,
  type RuntimeSessionView,
  type UpsertNotificationRouteRequestDto,
  type UpsertProviderCredentialRequestDto,
} from "@/src/lib/cadence-model"
import { getCloudProviderPreset } from "@/src/lib/cadence-model/provider-presets"
import { type OnboardingStepId } from "./types"

const STEP_ORDER: Array<{ id: OnboardingStepId; showIndicator: boolean }> = [
  { id: "welcome", showIndicator: false },
  { id: "providers", showIndicator: true },
  { id: "project", showIndicator: true },
  { id: "notifications", showIndicator: true },
  { id: "confirm", showIndicator: true },
]

const INDICATOR_STEPS = STEP_ORDER.filter((step) => step.showIndicator)

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
  providerModelCatalogs: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses: Record<string, ProviderModelCatalogLoadStatus>
  runtimeSession: RuntimeSessionView | null
  project: ImportedProjectView | null
  isImporting: boolean
  isProjectLoading: boolean
  projectErrorMessage: string | null
  notificationRoutes: NotificationRouteHealthView[]
  notificationRouteMutationStatus: NotificationRouteMutationStatus
  pendingNotificationRouteId: string | null
  notificationRouteMutationError: OperatorActionErrorView | null
  onImportProject: () => Promise<void>
  onRefreshProviderModelCatalog?: (
    profileId: string,
    options?: { force?: boolean },
  ) => Promise<ProviderModelCatalogDto>
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
  providerModelCatalogs,
  providerModelCatalogLoadStatuses,
  runtimeSession,
  project,
  isImporting,
  isProjectLoading,
  projectErrorMessage,
  notificationRoutes,
  notificationRouteMutationStatus,
  pendingNotificationRouteId,
  notificationRouteMutationError,
  onImportProject,
  onRefreshProviderModelCatalog,
  onRefreshProviderCredentials,
  onUpsertProviderCredential,
  onDeleteProviderCredential,
  onStartOAuthLogin,
  onUpsertNotificationRoute,
  onComplete,
  onDismiss,
}: OnboardingFlowProps) {
  const [stepIndex, setStepIndex] = useState(0)
  const directionRef = useRef<1 | -1>(1)

  const currentStep = STEP_ORDER[stepIndex]
  const providerReview = getProviderReview(providerCredentials, runtimeSession)

  const goTo = useCallback((target: number) => {
    setStepIndex((current) => {
      const clamped = Math.max(0, Math.min(STEP_ORDER.length - 1, target))
      directionRef.current = clamped >= current ? 1 : -1
      return clamped
    })
  }, [])

  const next = useCallback(() => goTo(stepIndex + 1), [goTo, stepIndex])
  const back = useCallback(() => goTo(stepIndex - 1), [goTo, stepIndex])

  const indicatorIndex = useMemo(
    () => Math.max(0, INDICATOR_STEPS.findIndex((step) => step.id === currentStep.id)),
    [currentStep.id],
  )

  const showFooter = currentStep.id !== "welcome"
  const isConfirm = currentStep.id === "confirm"
  const primaryLabel = isConfirm ? "Enter Cadence" : "Continue"
  const handlePrimary = isConfirm ? onComplete : next

  return (
    <div className="relative flex min-h-full flex-1 flex-col overflow-hidden bg-background text-foreground">
      <header className="relative z-10 flex shrink-0 items-center justify-between gap-3 px-8 pt-5">
        <div className="min-w-[72px]">
          {currentStep.showIndicator ? (
            <StepIndicator total={INDICATOR_STEPS.length} currentIndex={indicatorIndex} />
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
                  onClick={next}
                  className="h-8 text-[12px] text-muted-foreground hover:text-foreground"
                >
                  Skip
                </Button>
              ) : null}
              <Button
                size="sm"
                onClick={handlePrimary}
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
