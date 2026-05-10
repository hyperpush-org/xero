'use client'

import { useEffect, useState } from 'react'
import { ArrowLeft, ChevronRight, FolderOpen, FolderPlus, Loader2, Sparkles } from 'lucide-react'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'

type Mode = 'choose' | 'create'

export interface ProjectAddDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  isImporting: boolean
  onSelectExisting: () => Promise<boolean | void> | boolean | void
  onPickParentFolder: () => Promise<string | null>
  onCreate: (parentPath: string, name: string) => Promise<boolean>
}

export function ProjectAddDialog({
  open,
  onOpenChange,
  isImporting,
  onSelectExisting,
  onPickParentFolder,
  onCreate,
}: ProjectAddDialogProps) {
  const [mode, setMode] = useState<Mode>('choose')
  const [name, setName] = useState('')
  const [parentPath, setParentPath] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    if (open) {
      setMode('choose')
      setName('')
      setParentPath(null)
      setError(null)
      setBusy(false)
    }
  }, [open])

  const trimmedName = name.trim()
  const canCreate = trimmedName.length > 0 && parentPath !== null && !busy

  const handleSelectExisting = async () => {
    setBusy(true)
    setError(null)
    try {
      await onSelectExisting()
      onOpenChange(false)
    } finally {
      setBusy(false)
    }
  }

  const handlePickParent = async () => {
    setError(null)
    try {
      const picked = await onPickParentFolder()
      if (picked) {
        setParentPath(picked)
      }
    } catch (pickError) {
      setError(pickError instanceof Error ? pickError.message : 'Could not pick a folder.')
    }
  }

  const submitCreate = async () => {
    if (!parentPath) {
      setError('Pick a parent folder first.')
      return
    }
    if (!trimmedName) {
      setError('Project name cannot be empty.')
      return
    }
    if (trimmedName.includes('/') || trimmedName.includes('\\')) {
      setError('Project name cannot contain slashes.')
      return
    }

    setBusy(true)
    setError(null)
    try {
      const ok = await onCreate(parentPath, trimmedName)
      if (ok) {
        onOpenChange(false)
      } else {
        setError('Could not create the project. Check the project rail for details.')
      }
    } finally {
      setBusy(false)
    }
  }

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === 'Enter' && canCreate) {
      event.preventDefault()
      void submitCreate()
    }
  }

  const dialogBusy = busy || isImporting

  return (
    <Dialog open={open} onOpenChange={(next) => !dialogBusy && onOpenChange(next)}>
      <DialogContent className="gap-0 overflow-hidden p-0 sm:max-w-[460px]">
        <div
          aria-hidden
          className="pointer-events-none absolute inset-x-0 top-0 h-32 bg-gradient-to-b from-primary/[0.06] to-transparent"
        />

        <div className="relative px-6 pb-2 pt-6">
          <DialogHeader className="space-y-2">
            <div className="flex items-center gap-2.5">
              <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-primary/30 bg-primary/10 text-primary">
                <Sparkles className="h-4 w-4" />
              </span>
              <DialogTitle className="text-[15px]">Add a project</DialogTitle>
            </div>
            <DialogDescription className="text-[12.5px] leading-relaxed">
              {mode === 'choose'
                ? 'Open an existing repository or scaffold a brand-new project.'
                : 'Choose where the new project folder should live and give it a name.'}
            </DialogDescription>
          </DialogHeader>
        </div>

        <div className="relative px-6 pb-5">
          {mode === 'choose' ? (
            <div className="flex flex-col gap-2">
              <ChoiceCard
                icon={<FolderOpen className="h-4 w-4" />}
                title="Open existing"
                description="Pick a folder that already contains a Git repository."
                disabled={dialogBusy}
                loading={dialogBusy}
                onClick={() => void handleSelectExisting()}
              />
              <ChoiceCard
                icon={<FolderPlus className="h-4 w-4" />}
                title="Create new"
                description="Make a new folder and initialize it as a Git repository."
                disabled={dialogBusy}
                onClick={() => setMode('create')}
              />
            </div>
          ) : (
            <div className="space-y-4">
              <div className="space-y-1.5">
                <label
                  className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground/80"
                  htmlFor="project-name"
                >
                  Project name
                </label>
                <Input
                  autoFocus
                  id="project-name"
                  value={name}
                  onChange={(event) => {
                    setName(event.target.value)
                    setError(null)
                  }}
                  onKeyDown={onKeyDown}
                  placeholder="my-new-project"
                  disabled={dialogBusy}
                  className={cn(
                    'h-9 text-[13px]',
                    error && 'border-destructive focus-visible:ring-destructive/30',
                  )}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground/80">
                  Parent folder
                </label>
                <div className="flex items-center gap-2">
                  <div
                    className={cn(
                      'flex h-9 min-w-0 flex-1 items-center truncate rounded-md border border-input bg-secondary/30 px-2.5 font-mono text-[12px]',
                      parentPath ? 'text-foreground' : 'text-muted-foreground/70',
                    )}
                    title={parentPath ?? undefined}
                  >
                    {parentPath ?? 'No folder selected'}
                  </div>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => void handlePickParent()}
                    disabled={dialogBusy}
                    className="h-9 shrink-0"
                  >
                    <FolderOpen className="h-3.5 w-3.5" />
                    Pick
                  </Button>
                </div>
                {parentPath ? (
                  <p className="pt-0.5 font-mono text-[11px] text-muted-foreground/70">
                    Will create at{' '}
                    <span className="text-foreground/80">
                      {trimmedName ? `${parentPath}/${trimmedName}` : `${parentPath}/…`}
                    </span>
                  </p>
                ) : null}
              </div>
              {error ? (
                <p className="rounded-md border border-destructive/40 bg-destructive/10 px-2.5 py-1.5 text-[12px] text-destructive">
                  {error}
                </p>
              ) : null}
            </div>
          )}
        </div>

        <DialogFooter className="border-t border-border/60 bg-secondary/20 px-6 py-3 sm:justify-between">
          {mode === 'choose' ? (
            <>
              <p className="hidden text-[11px] text-muted-foreground/70 sm:block">
                Projects stay local — Xero never uploads your code.
              </p>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => onOpenChange(false)}
                disabled={dialogBusy}
                className="text-muted-foreground hover:text-foreground"
              >
                Cancel
              </Button>
            </>
          ) : (
            <>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  setMode('choose')
                  setError(null)
                }}
                disabled={dialogBusy}
                className="text-muted-foreground hover:text-foreground"
              >
                <ArrowLeft className="h-3.5 w-3.5" />
                Back
              </Button>
              <Button
                size="sm"
                onClick={() => void submitCreate()}
                disabled={!canCreate || dialogBusy}
              >
                {dialogBusy ? (
                  <>
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    Creating…
                  </>
                ) : (
                  'Create project'
                )}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

interface ChoiceCardProps {
  icon: React.ReactNode
  title: string
  description: string
  disabled?: boolean
  loading?: boolean
  onClick: () => void
}

function ChoiceCard({ icon, title, description, disabled, loading, onClick }: ChoiceCardProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        'group relative flex items-center gap-3 rounded-lg border border-border/60 bg-card/40 px-3.5 py-3 text-left transition-all',
        'hover:border-primary/40 hover:bg-primary/[0.04]',
        'focus-visible:border-primary/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/30',
        'disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:border-border/60 disabled:hover:bg-card/40',
      )}
    >
      <span
        className={cn(
          'flex h-9 w-9 shrink-0 items-center justify-center rounded-md border transition-colors',
          'border-border/60 bg-secondary/60 text-muted-foreground',
          'group-hover:border-primary/40 group-hover:bg-primary/10 group-hover:text-primary',
          'group-disabled:group-hover:border-border/60 group-disabled:group-hover:bg-secondary/60 group-disabled:group-hover:text-muted-foreground',
        )}
      >
        {loading ? <Loader2 className="h-4 w-4 animate-spin text-primary" /> : icon}
      </span>
      <div className="min-w-0 flex-1 space-y-0.5">
        <div className="text-[13px] font-medium text-foreground">{title}</div>
        <div className="text-[11.5px] leading-snug text-muted-foreground">{description}</div>
      </div>
      <ChevronRight
        className={cn(
          'h-4 w-4 shrink-0 text-muted-foreground/50 transition-all',
          'group-hover:translate-x-0.5 group-hover:text-primary',
        )}
      />
    </button>
  )
}
