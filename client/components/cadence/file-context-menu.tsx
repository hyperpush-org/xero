"use client"

import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { File, FileEdit, FolderOpen, Trash2, Copy, Plus } from 'lucide-react'
import { cn } from '@/lib/utils'

export interface FileContextMenuProps {
  path: string
  type: 'file' | 'folder'
  onRename?: () => void
  onDelete?: () => void
  onNewFile?: () => void
  onNewFolder?: () => void
  onCopyPath?: () => void
  children: React.ReactNode
}

export function FileContextMenu({
  path,
  type,
  onRename,
  onDelete,
  onNewFile,
  onNewFolder,
  onCopyPath,
  children,
}: FileContextMenuProps) {
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="w-48">
        {type === 'file' && (
          <>
            {onRename && (
              <ContextMenuItem onClick={onRename}>
                <FileEdit className="h-4 w-4 mr-2" />
                Rename
              </ContextMenuItem>
            )}
            {onCopyPath && (
              <ContextMenuItem onClick={onCopyPath}>
                <Copy className="h-4 w-4 mr-2" />
                Copy Path
              </ContextMenuItem>
            )}
          </>
        )}
        {type === 'folder' && (
          <>
            {onNewFile && (
              <ContextMenuItem onClick={onNewFile}>
                <Plus className="h-4 w-4 mr-2" />
                New File
              </ContextMenuItem>
            )}
            {onNewFolder && (
              <ContextMenuItem onClick={onNewFolder}>
                <FolderOpen className="h-4 w-4 mr-2" />
                New Folder
              </ContextMenuItem>
            )}
            {onRename && (
              <ContextMenuItem onClick={onRename}>
                <FileEdit className="h-4 w-4 mr-2" />
                Rename
              </ContextMenuItem>
            )}
          </>
        )}
        <ContextMenuSeparator />
        {onDelete && (
          <ContextMenuItem onClick={onDelete} className="text-destructive">
            <Trash2 className="h-4 w-4 mr-2" />
            Delete
          </ContextMenuItem>
        )}
      </ContextMenuContent>
    </ContextMenu>
  )
}
