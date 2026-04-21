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
import { File, Folder } from 'lucide-react'
import { cn } from '@/lib/utils'

interface RenameFileDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  currentPath: string
  type: 'file' | 'folder'
  onRename: (newName: string) => string | null | void | Promise<string | null | void>
}

export function RenameFileDialog({
  open,
  onOpenChange,
  currentPath,
  type,
  onRename,
}: RenameFileDialogProps) {
  const [newName, setNewName] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)

  const currentName = currentPath.split('/').pop() ?? ''

  useEffect(() => {
    if (open) {
      setNewName(currentName)
      setError(null)
      setIsSubmitting(false)
    }
  }, [open, currentName])

  const submit = async () => {
    const trimmed = newName.trim()
    if (!trimmed) {
      setError('Name cannot be empty')
      return
    }
    if (trimmed.includes('/') || trimmed.includes('\\')) {
      setError('Name cannot contain slashes')
      return
    }
    if (trimmed === currentName) {
      onOpenChange(false)
      return
    }

    setIsSubmitting(true)
    try {
      const result = await onRename(trimmed)
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

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <div className="flex items-center gap-2">
            {type === 'folder' ? (
              <Folder className="h-5 w-5 text-muted-foreground" />
            ) : (
              <File className="h-5 w-5 text-muted-foreground" />
            )}
            <DialogTitle>Rename {type}</DialogTitle>
          </div>
          <DialogDescription>
            Enter a new name for <span className="font-mono">{currentName}</span>
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-2 py-2">
          <Input
            autoFocus
            value={newName}
            onChange={(event) => {
              setNewName(event.target.value)
              setError(null)
            }}
            onKeyDown={onKeyDown}
            placeholder="Enter new name"
            className={cn(error && 'border-destructive focus-visible:ring-destructive/30')}
          />
          {error ? <p className="text-[12px] text-destructive">{error}</p> : null}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={() => void submit()} disabled={isSubmitting || !newName.trim()}>
            {isSubmitting ? 'Renaming…' : 'Rename'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
