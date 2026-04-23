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
  ProviderModelCatalogLoadStatus,
  ProviderProfilesLoadStatus,
  ProviderProfilesSaveStatus,
} from "@/src/features/cadence/use-cadence-desktop-state"
import {
  getActiveProviderProfile,
  type ProviderModelCatalogDto,
  type ProviderProfilesDto,
  type RuntimeSessionView,
  type UpsertNotificationRouteRequestDto,
  type UpsertProviderProfileRequestDto,
} from "@/src/lib/cadence-model"
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

function getProviderReview(providerProfiles: ProviderProfilesDto | null, runtimeSession: RuntimeSessionView | null) {
  const activeProfile = getActiveProviderProfile(providerProfiles)
  if (!activeProfile) {
    return {
      ready: false,
      value: "No provider set up yet",
    }
  }

  if (activeProfile.providerId === "openrouter" || activeProfile.providerId === "anthropic") {
    return activeProfile.readiness.ready
      ? {
          ready: true,
          value: `${activeProfile.label} · API key saved`,
        }
      : {
          ready: false,
          value: `${activeProfile.label} · API key required`,
        }
  }

  const isOpenAiConnected = Boolean(runtimeSession?.providerId === "openai_codex" && runtimeSession.isAuthenticated)

  return isOpenAiConnected
    ? {
        ready: true,
        value: `${activeProfile.label} · connected`,
      }
    : {
        ready: true,
        value: `${activeProfile.label} · active profile`,
      }
}

export interface OnboardingFlowProps {
  providerProfiles: ProviderProfilesDto | null
  providerProfilesLoadStatus: ProviderProfilesLoadStatus
  providerProfilesLoadError: OperatorActionErrorView | null
  providerProfilesSaveStatus: ProviderProfilesSaveStatus
  providerProfilesSaveError: OperatorActionErrorView | null
  providerModelCatalogs: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses: Record<string, ProviderModelCatalogLoadStatus>
  providerModelCatalogLoadErrors: Record<string, OperatorActionErrorView | null>
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
  onRefreshProviderProfiles?: (options?: { force?: boolean }) => Promise<ProviderProfilesDto>
  onRefreshProviderModelCatalog?: (
    profileId: string,
    options?: { force?: boolean },
  ) => Promise<ProviderModelCatalogDto>
  onUpsertProviderProfile: (request: UpsertProviderProfileRequestDto) => Promise<ProviderProfilesDto>
  onSetActiveProviderProfile: (profileId: string) => Promise<ProviderProfilesDto>
  onStartLogin?: () => Promise<RuntimeSessionView | null>
  onLogout?: () => Promise<RuntimeSessionView | null>
  onUpsertNotificationRoute: (
    request: Omit<UpsertNotificationRouteRequestDto, "projectId">,
  ) => Promise<unknown>
  onComplete: () => void
  onDismiss: () => void
}

export function OnboardingFlow({
  providerProfiles,
  providerProfilesLoadStatus,
  providerProfilesLoadError,
  providerProfilesSaveStatus,
  providerProfilesSaveError,
  providerModelCatalogs,
  providerModelCatalogLoadStatuses,
  providerModelCatalogLoadErrors,
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
  onRefreshProviderProfiles,
  onRefreshProviderModelCatalog,
  onUpsertProviderProfile,
  onSetActiveProviderProfile,
  onStartLogin,
  onLogout,
  onUpsertNotificationRoute,
  onComplete,
  onDismiss,
}: OnboardingFlowProps) {
  const [stepIndex, setStepIndex] = useState(0)
  const directionRef = useRef<1 | -1>(1)

  const currentStep = STEP_ORDER[stepIndex]
  const providerReview = getProviderReview(providerProfiles, runtimeSession)

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

      <main className="relative z-10 flex flex-1 items-center justify-center overflow-y-auto px-8 py-10">
        <div
          key={currentStep.id}
          className={`w-full max-w-md animate-in fade-in-0 duration-200 ease-out ${
            directionRef.current === 1 ? "slide-in-from-right-4" : "slide-in-from-left-4"
          }`}
        >
          {currentStep.id === "welcome" ? (
            <WelcomeStep onContinue={next} onSkipAll={onDismiss} />
          ) : null}
          {currentStep.id === "providers" ? (
            <ProvidersStep
              providerProfiles={providerProfiles}
              providerProfilesLoadStatus={providerProfilesLoadStatus}
              providerProfilesLoadError={providerProfilesLoadError}
              providerProfilesSaveStatus={providerProfilesSaveStatus}
              providerProfilesSaveError={providerProfilesSaveError}
              providerModelCatalogs={providerModelCatalogs}
              providerModelCatalogLoadStatuses={providerModelCatalogLoadStatuses}
              providerModelCatalogLoadErrors={providerModelCatalogLoadErrors}
              runtimeSession={runtimeSession}
              hasSelectedProject={Boolean(project?.path?.trim())}
              onRefreshProviderProfiles={onRefreshProviderProfiles}
              onRefreshProviderModelCatalog={onRefreshProviderModelCatalog}
              onUpsertProviderProfile={onUpsertProviderProfile}
              onSetActiveProviderProfile={onSetActiveProviderProfile}
              onStartLogin={onStartLogin}
              onLogout={onLogout}
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
      </main>

      {showFooter ? (
        <footer className="relative z-10 shrink-0">
          <div className="mx-auto flex w-full max-w-md items-center justify-between gap-2 px-8 pb-6">
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
