import { useCallback, useMemo, useState } from "react"
import { Loader2, Trash2 } from "lucide-react"

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type {
  WipeAllDataResponseDto,
  WipeProjectDataResponseDto,
} from "@/src/lib/xero-model/wipe-data"

import { ErrorBanner, SubHeading, SuccessBanner } from "./_shared"

const WIPE_ALL_CONFIRMATION = "WIPE ALL"

export interface DangerZoneProject {
  id: string
  name: string
}

export interface DangerSettingsAdapter {
  wipeProject: (projectId: string) => Promise<WipeProjectDataResponseDto>
  wipeAll: () => Promise<WipeAllDataResponseDto>
}

interface AccountDangerZoneProps {
  projects: DangerZoneProject[]
  activeProjectId: string | null
  adapter?: DangerSettingsAdapter | null
}

type PendingKind = "project" | "all"

export function AccountDangerZone({ projects, activeProjectId, adapter }: AccountDangerZoneProps) {
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(
    activeProjectId ?? projects[0]?.id ?? null,
  )
  const [confirmProjectOpen, setConfirmProjectOpen] = useState(false)
  const [confirmAllOpen, setConfirmAllOpen] = useState(false)
  const [confirmAllInput, setConfirmAllInput] = useState("")
  const [pending, setPending] = useState<PendingKind | null>(null)
  const [message, setMessage] = useState<string | null>(null)
  const [errorText, setErrorText] = useState<string | null>(null)

  const projectOptions = useMemo(
    () =>
      projects.map((project) => ({
        id: project.id,
        label: project.name && project.name.length > 0 ? project.name : project.id,
      })),
    [projects],
  )

  const targetProject = useMemo(
    () => projects.find((project) => project.id === selectedProjectId) ?? null,
    [projects, selectedProjectId],
  )

  const handleSelectProject = useCallback((value: string) => {
    setSelectedProjectId(value)
  }, [])

  const handleWipeProject = useCallback(async () => {
    if (!adapter || !selectedProjectId) return
    setPending("project")
    setMessage(null)
    setErrorText(null)
    try {
      const response = await adapter.wipeProject(selectedProjectId)
      const removedLabel = response.directoryRemoved
        ? "Removed on-disk data."
        : "No on-disk data found."
      setMessage(`Wiped project ${selectedProjectId}. ${removedLabel}`)
      setSelectedProjectId(null)
    } catch (error) {
      setErrorText(errorMessage(error, "Xero could not wipe project data."))
    } finally {
      setPending(null)
      setConfirmProjectOpen(false)
    }
  }, [adapter, selectedProjectId])

  const handleWipeAll = useCallback(async () => {
    if (!adapter) return
    setPending("all")
    setMessage(null)
    setErrorText(null)
    try {
      const response = await adapter.wipeAll()
      const removedLabel = response.directoryRemoved
        ? "App-data directory cleared."
        : "App-data directory was already empty."
      setMessage(
        `All Xero data wiped. ${removedLabel} Restart Xero so it can reinitialize from a clean state.`,
      )
      setSelectedProjectId(null)
    } catch (error) {
      setErrorText(errorMessage(error, "Xero could not wipe all data."))
    } finally {
      setPending(null)
      setConfirmAllOpen(false)
      setConfirmAllInput("")
    }
  }, [adapter])

  const wipeAllConfirmed = confirmAllInput.trim() === WIPE_ALL_CONFIRMATION

  return (
    <section
      aria-labelledby="account-danger-zone-heading"
      className="flex flex-col gap-3"
    >
      <div className="flex flex-col gap-1">
        <SubHeading
          id="account-danger-zone-heading"
          className="text-destructive"
        >
          Danger zone
        </SubHeading>
        <p className="text-[12px] leading-[1.5] text-muted-foreground">
          Destructive actions tied to this Xero install. These changes cannot be undone.
        </p>
      </div>

      {!adapter ? (
        <ErrorBanner message="The desktop adapter did not expose wipe commands. Restart Xero to enable this surface." />
      ) : null}

      {message ? <SuccessBanner message={message} testId="danger-action-message" /> : null}
      {errorText ? <ErrorBanner message={errorText} /> : null}

      <div className="overflow-hidden rounded-md border border-border/60 divide-y divide-border/40">
        <DangerRow
          title="Wipe a project's data"
          description="Removes the SQLite store, vector index, code-history, and backups for one project. The source repository on disk is left untouched."
          testId="danger-wipe-project"
        >
          <Select
            value={selectedProjectId ?? undefined}
            onValueChange={handleSelectProject}
            disabled={!adapter || projectOptions.length === 0 || pending !== null}
          >
            <SelectTrigger
              id="danger-project-select"
              className="h-8 w-[160px] text-[12.5px]"
              aria-label="Project to wipe"
            >
              <SelectValue
                placeholder={
                  projectOptions.length === 0 ? "No projects" : "Select project"
                }
              />
            </SelectTrigger>
            <SelectContent>
              {projectOptions.map((option) => (
                <SelectItem key={option.id} value={option.id}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            type="button"
            variant="destructive"
            size="sm"
            className="h-8 gap-1.5 text-[12.5px]"
            onClick={() => setConfirmProjectOpen(true)}
            disabled={!adapter || !selectedProjectId || pending !== null}
            data-testid="danger-wipe-project-trigger"
          >
            {pending === "project" ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Trash2 className="h-3.5 w-3.5" />
            )}
            Wipe project
          </Button>
        </DangerRow>

        <DangerRow
          title="Wipe ALL Xero data"
          description="Deletes the entire Xero app-data directory: project registry, every SQLite + Lance store, every backup, UI state, and credentials. Xero starts blank."
          testId="danger-wipe-all"
        >
          <Button
            type="button"
            variant="destructive"
            size="sm"
            className="h-8 gap-1.5 text-[12.5px]"
            onClick={() => {
              setConfirmAllInput("")
              setConfirmAllOpen(true)
            }}
            disabled={!adapter || pending !== null}
            data-testid="danger-wipe-all-trigger"
          >
            {pending === "all" ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Trash2 className="h-3.5 w-3.5" />
            )}
            Wipe everything
          </Button>
        </DangerRow>
      </div>

      <AlertDialog
        open={confirmProjectOpen}
        onOpenChange={(open) => {
          if (pending === "project") return
          setConfirmProjectOpen(open)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Wipe project data?</AlertDialogTitle>
            <AlertDialogDescription>
              This deletes every Xero record for{" "}
              <span className="font-medium text-foreground">
                {targetProject?.name ?? selectedProjectId ?? "this project"}
              </span>
              : SQLite store, vector index, code-history, and backups. The source repository on
              disk is not touched. This cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={pending === "project"}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={(event) => {
                event.preventDefault()
                void handleWipeProject()
              }}
              disabled={pending === "project"}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {pending === "project" ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Trash2 className="h-3.5 w-3.5" />
              )}
              Wipe project
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog
        open={confirmAllOpen}
        onOpenChange={(open) => {
          if (pending === "all") return
          setConfirmAllOpen(open)
          if (!open) setConfirmAllInput("")
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Wipe ALL Xero data?</AlertDialogTitle>
            <AlertDialogDescription>
              This deletes the entire Xero app-data directory, including every project's SQLite +
              Lance store, every backup, UI state, and stored credentials. Source repositories on
              disk are not touched, but Xero will lose every reference to them. Type{" "}
              <span className="font-mono font-semibold text-foreground">
                {WIPE_ALL_CONFIRMATION}
              </span>{" "}
              to confirm.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <Input
            value={confirmAllInput}
            onChange={(event) => setConfirmAllInput(event.target.value)}
            placeholder={WIPE_ALL_CONFIRMATION}
            aria-label="Confirmation phrase"
            disabled={pending === "all"}
            className="mt-2"
            data-testid="danger-wipe-all-input"
          />
          <AlertDialogFooter>
            <AlertDialogCancel disabled={pending === "all"}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={(event) => {
                event.preventDefault()
                void handleWipeAll()
              }}
              disabled={pending === "all" || !wipeAllConfirmed}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {pending === "all" ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Trash2 className="h-3.5 w-3.5" />
              )}
              Wipe everything
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </section>
  )
}

interface DangerRowProps {
  title: string
  description: string
  testId?: string
  children: React.ReactNode
}

function DangerRow({ title, description, testId, children }: DangerRowProps) {
  return (
    <div
      data-testid={testId}
      className="flex flex-wrap items-center gap-x-4 gap-y-3 px-3.5 py-3"
    >
      <div className="min-w-[260px] flex-1">
        <p className="text-[12.5px] font-semibold text-foreground">{title}</p>
        <p className="mt-0.5 text-[11.5px] leading-[1.5] text-muted-foreground">
          {description}
        </p>
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  )
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback
}
