import { useSyncExternalStore } from 'react'

export interface ProjectSelectionPreviewSnapshot {
  projectId: string | null
  projectName: string | null
}

const EMPTY_PROJECT_SELECTION_PREVIEW: ProjectSelectionPreviewSnapshot = {
  projectId: null,
  projectName: null,
}

let snapshot: ProjectSelectionPreviewSnapshot = EMPTY_PROJECT_SELECTION_PREVIEW
const listeners = new Set<() => void>()

function emit(nextSnapshot: ProjectSelectionPreviewSnapshot): void {
  if (
    snapshot.projectId === nextSnapshot.projectId &&
    snapshot.projectName === nextSnapshot.projectName
  ) {
    return
  }

  snapshot = nextSnapshot
  listeners.forEach((listener) => listener())
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function getSnapshot(): ProjectSelectionPreviewSnapshot {
  return snapshot
}

export function previewProjectSelection(projectId: string, projectName: string): void {
  const trimmedProjectName = projectName.trim()
  if (!projectId || !trimmedProjectName) {
    return
  }

  emit({
    projectId,
    projectName: trimmedProjectName,
  })
}

export function clearProjectSelectionPreview(projectId?: string | null): void {
  if (projectId && snapshot.projectId !== projectId) {
    return
  }

  emit(EMPTY_PROJECT_SELECTION_PREVIEW)
}

export function useProjectSelectionPreview(): ProjectSelectionPreviewSnapshot {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
}

export function getProjectSelectionPreviewSnapshotForTests(): ProjectSelectionPreviewSnapshot {
  return snapshot
}
