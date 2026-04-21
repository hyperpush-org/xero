"use client"

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { AlertTriangle, File, Folder } from 'lucide-react'

interface DeleteFileDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  path: string
  type: 'file' | 'folder'
  onDelete: () => void
}

export function DeleteFileDialog({ open, onOpenChange, path, type, onDelete }: DeleteFileDialogProps) {
  const fileName = path.split('/').pop() ?? ''

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <div className="flex items-center gap-2 text-destructive">
            <AlertTriangle className="h-5 w-5" />
            <DialogTitle>Delete {type}</DialogTitle>
          </div>
          <DialogDescription>
            Are you sure you want to delete <span className="font-mono">{fileName}</span>? This action cannot be undone.
          </DialogDescription>
        </DialogHeader>
        <div className="py-4">
          <div className="flex items-start gap-3 rounded-md border border-border bg-muted/50 p-3">
            {type === 'file' ? (
              <File className="h-5 w-5 mt-0.5 text-muted-foreground" />
            ) : (
              <Folder className="h-5 w-5 mt-0.5 text-muted-foreground" />
            )}
            <div className="flex-1 min-w-0">
              <p className="font-mono text-sm truncate">{path}</p>
            </div>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={onDelete}>
            Delete
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
