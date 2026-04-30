"use client"

import { useEffect, useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { FilePlus, FolderPlus } from 'lucide-react'
import { cn } from '@/lib/utils'

interface NewFileDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  parentPath: string
  type: 'file' | 'folder'
  onCreate: (name: string) => string | null | void | Promise<string | null | void>
}

export function NewFileDialog({ open, onOpenChange, parentPath, type, onCreate }: NewFileDialogProps) {
  const [name, setName] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)

  useEffect(() => {
    if (open) {
      setName('')
      setError(null)
      setIsSubmitting(false)
    }
  }, [open])

  const submit = async () => {
    const trimmed = name.trim()
    if (!trimmed) {
      setError('Name cannot be empty')
      return
    }
    if (trimmed.includes('/') || trimmed.includes('\\')) {
      setError('Name cannot contain slashes')
      return
    }

    setIsSubmitting(true)
    try {
      const result = await onCreate(trimmed)
      if (typeof result === 'string' && result) {
        setError(result)
        return
      }
      onOpenChange(false)
    } finally {
      setIsSubmitting(false)
    }
  }

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === 'Enter') {
      event.preventDefault()
      void submit()
    }
  }

  const Icon = type === 'folder' ? FolderPlus : FilePlus
  const placeholder = type === 'file' ? 'filename.ext' : 'folder-name'

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <div className="flex items-center gap-2">
            <Icon className="h-5 w-5 text-muted-foreground" />
            <DialogTitle>New {type}</DialogTitle>
          </div>
          <DialogDescription>
            Create a new {type} inside{' '}
            <span className="font-mono">{parentPath === '/' ? '/' : parentPath}</span>
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-2 py-2">
          <Input
            autoFocus
            value={name}
            onChange={(event) => {
              setName(event.target.value)
              setError(null)
            }}
            onKeyDown={onKeyDown}
            placeholder={placeholder}
            className={cn(error && 'border-destructive focus-visible:ring-destructive/30')}
          />
          {error ? <p className="text-[12px] text-destructive">{error}</p> : null}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={() => void submit()} disabled={isSubmitting || !name.trim()}>
            {isSubmitting ? 'Creating…' : 'Create'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
