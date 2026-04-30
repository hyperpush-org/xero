"use client"

import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { FileEdit, FolderOpen, Trash2, Copy, Plus } from 'lucide-react'

export interface FileContextMenuProps {
  type: 'file' | 'folder'
  onRename?: () => void
  onDelete?: () => void
  onNewFile?: () => void
  onNewFolder?: () => void
  onCopyPath?: () => void
  children: React.ReactNode
}

export function FileContextMenu({
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
        {onDelete ? (
          <>
            <ContextMenuSeparator />
            <ContextMenuItem onClick={onDelete} className="text-destructive">
              <Trash2 className="h-4 w-4 mr-2" />
              Delete
            </ContextMenuItem>
          </>
        ) : null}
      </ContextMenuContent>
    </ContextMenu>
  )
}
