import type {
  RuntimeStreamActivityItemView,
  RuntimeStreamToolItemView,
  RuntimeStreamViewItem,
} from '@/src/lib/xero-model'
import type { CodePatchAvailabilityDto } from '@/src/lib/xero-model/code-history'

const MAX_PROMPT_TEXT_CHARS = 8_000
const CODE_EDIT_TOOL_NAMES = new Set(['edit', 'patch', 'write', 'apply_patch', 'notebook_edit'])

export interface EditorSelectionContext {
  text: string
  fromLine: number
  fromColumn: number
  toLine: number
  toColumn: number
}

export type EditorAgentContextKind = 'ask_selection' | 'fix_file'

export interface EditorAgentContextRequest {
  kind: EditorAgentContextKind
  prompt: string
  path: string
  content: string
  savedContent: string
  isDirty: boolean
  selection: EditorSelectionContext | null
}

export type EditorAgentActivityStatus = 'active' | 'pending' | 'recent'

export interface EditorAgentActivity {
  id: string
  path: string
  operation: string
  status: EditorAgentActivityStatus
  title: string
  detail: string
  sessionTitle: string | null
  paneId: string | null
  runId: string | null
  createdAt: string | null
  sequence: number
  changeGroupId: string | null
  workspaceEpoch: number | null
  patchAvailability: CodePatchAvailabilityDto | null
}

export interface EditorAgentActivitySource {
  paneId?: string | null
  sessionTitle?: string | null
  runtimeStreamItems?: readonly RuntimeStreamViewItem[] | null
}

export function truncatePromptText(value: string, maxChars = MAX_PROMPT_TEXT_CHARS): string {
  if (value.length <= maxChars) return value
  const omitted = value.length - maxChars
  return `${value.slice(0, maxChars)}\n\n[... ${omitted.toLocaleString()} characters omitted ...]`
}

export function buildEditorAgentContextRequest({
  kind,
  path,
  content,
  savedContent,
  isDirty,
  selection,
}: {
  kind: EditorAgentContextKind
  path: string
  content: string
  savedContent: string
  isDirty: boolean
  selection: EditorSelectionContext | null
}): EditorAgentContextRequest {
  const prompt = kind === 'ask_selection'
    ? buildAskSelectionPrompt({ path, content, isDirty, selection })
    : buildFixFilePrompt({ path, content, isDirty, selection })

  return {
    kind,
    prompt,
    path,
    content,
    savedContent,
    isDirty,
    selection,
  }
}

function buildAskSelectionPrompt({
  path,
  content,
  isDirty,
  selection,
}: {
  path: string
  content: string
  isDirty: boolean
  selection: EditorSelectionContext | null
}): string {
  const lines = [
    `Please explain this editor selection from ${path}.`,
    isDirty
      ? 'Important: the editor has unsaved changes, so the selection below is from my current draft rather than disk.'
      : null,
  ].filter((line): line is string => Boolean(line))

  if (selection?.text.trim()) {
    lines.push(
      `Selection: lines ${selection.fromLine}:${selection.fromColumn} to ${selection.toLine}:${selection.toColumn}.`,
      '```',
      truncatePromptText(selection.text),
      '```',
    )
  } else {
    lines.push(
      'There is no active selection; use the active file context below.',
      '```',
      truncatePromptText(content),
      '```',
    )
  }

  return lines.join('\n')
}

function buildFixFilePrompt({
  path,
  content,
  isDirty,
  selection,
}: {
  path: string
  content: string
  isDirty: boolean
  selection: EditorSelectionContext | null
}): string {
  const hasSelection = Boolean(selection?.text.trim())
  const lines = [
    `Please fix ${hasSelection ? 'this selection in' : 'this file'} ${path}.`,
    'Keep the change focused and preserve the surrounding style.',
    isDirty
      ? 'Important: the editor has unsaved changes. Treat the draft content below as the user-visible source of truth before editing disk.'
      : 'Read the current project file before editing so your patch is based on disk.',
  ]

  if (hasSelection && selection) {
    lines.push(
      `Target selection: lines ${selection.fromLine}:${selection.fromColumn} to ${selection.toLine}:${selection.toColumn}.`,
      '```',
      truncatePromptText(selection.text),
      '```',
    )
  } else {
    lines.push('Current editor draft:', '```', truncatePromptText(content), '```')
  }

  return lines.join('\n')
}

export function normalizeAgentActivityPath(value: string): string | null {
  const trimmed = value
    .trim()
    .replace(/^["'`]+|["'`]+$/g, '')
    .replace(/\\/g, '/')
    .replace(/^\.\//, '')
    .replace(/^a\//, '')
    .replace(/^b\//, '')

  if (!trimmed || trimmed === '.' || trimmed.includes('\0') || trimmed.startsWith('..')) {
    return null
  }

  const withoutLeadingSlash = trimmed.replace(/^\/+/, '')
  return withoutLeadingSlash ? `/${withoutLeadingSlash}` : null
}

export function parseAgentFileActivityDetail(detail: string | null | undefined): {
  operation: string
  paths: string[]
} {
  if (!detail) {
    return { operation: 'changed', paths: [] }
  }

  const summary = detail.split(' · ')[0]?.trim() ?? detail.trim()
  const match = /^([^:]+):\s*(.+)$/.exec(summary)
  const operation = match?.[1]?.trim() || 'changed'
  const pathSegment = match?.[2]?.trim() || summary
  const paths = pathSegment
    .split(/\s+->\s+|\s*,\s*/)
    .map(normalizeAgentActivityPath)
    .filter((path): path is string => Boolean(path))

  return { operation, paths: Array.from(new Set(paths)) }
}

export function buildEditorAgentActivities(
  sources: readonly EditorAgentActivitySource[],
): EditorAgentActivity[] {
  const activities: EditorAgentActivity[] = []

  for (const source of sources) {
    const items = source.runtimeStreamItems ?? []
    for (const item of items) {
      if (isFileChangeActivityItem(item)) {
        activities.push(...activitiesFromFileChangeItem(item, source))
        continue
      }

      if (isPendingCodeEditToolItem(item)) {
        activities.push(...activitiesFromPendingToolItem(item, source))
      }
    }
  }

  const deduped = new Map<string, EditorAgentActivity>()
  for (const activity of activities) {
    const key = `${activity.id}\u0000${activity.path}`
    if (!deduped.has(key)) {
      deduped.set(key, activity)
    }
  }

  return Array.from(deduped.values()).sort((left, right) => {
    const rightTime = right.createdAt ? Date.parse(right.createdAt) : 0
    const leftTime = left.createdAt ? Date.parse(left.createdAt) : 0
    if (rightTime !== leftTime) return rightTime - leftTime
    return right.sequence - left.sequence
  })
}

export function countAgentActivitiesByPath(
  activities: readonly EditorAgentActivity[],
): Record<string, number> {
  const counts: Record<string, number> = {}
  for (const activity of activities) {
    counts[activity.path] = (counts[activity.path] ?? 0) + 1
  }
  return counts
}

function isFileChangeActivityItem(
  item: RuntimeStreamViewItem,
): item is RuntimeStreamActivityItemView {
  return item.kind === 'activity' && item.code === 'owned_agent_file_changed'
}

function isPendingCodeEditToolItem(item: RuntimeStreamViewItem): item is RuntimeStreamToolItemView {
  return (
    item.kind === 'tool' &&
    CODE_EDIT_TOOL_NAMES.has(item.toolName) &&
    (item.toolState === 'pending' || item.toolState === 'running')
  )
}

function activitiesFromFileChangeItem(
  item: RuntimeStreamActivityItemView,
  source: EditorAgentActivitySource,
): EditorAgentActivity[] {
  const parsed = parseAgentFileActivityDetail(item.detail ?? item.text)
  const paths = item.codePatchAvailability?.affectedPaths
    ?.map(normalizeAgentActivityPath)
    .filter((path): path is string => Boolean(path))
  const activityPaths = paths && paths.length > 0 ? Array.from(new Set(paths)) : parsed.paths

  return activityPaths.map((path) => ({
    id: item.id,
    path,
    operation: parsed.operation,
    status: 'recent' as const,
    title: item.title || 'Agent file change',
    detail: item.detail ?? item.text ?? `${parsed.operation}: ${path}`,
    sessionTitle: source.sessionTitle ?? null,
    paneId: source.paneId ?? null,
    runId: item.runId ?? null,
    createdAt: item.createdAt ?? null,
    sequence: item.sequence,
    changeGroupId: item.codeChangeGroupId ?? null,
    workspaceEpoch: item.codeWorkspaceEpoch ?? null,
    patchAvailability: item.codePatchAvailability ?? null,
  }))
}

function activitiesFromPendingToolItem(
  item: RuntimeStreamToolItemView,
  source: EditorAgentActivitySource,
): EditorAgentActivity[] {
  const parsed = parseAgentFileActivityDetail(item.detail ?? item.toolResultPreview)
  return parsed.paths.map((path) => ({
    id: item.id,
    path,
    operation: parsed.operation || item.toolName,
    status: item.toolState === 'running' ? 'active' as const : 'pending' as const,
    title: item.toolState === 'running' ? 'Agent editing file' : 'Agent edit pending',
    detail: item.detail ?? item.toolResultPreview ?? item.toolName,
    sessionTitle: source.sessionTitle ?? null,
    paneId: source.paneId ?? null,
    runId: item.runId ?? null,
    createdAt: item.createdAt ?? null,
    sequence: item.sequence,
    changeGroupId: item.codeChangeGroupId ?? null,
    workspaceEpoch: item.codeWorkspaceEpoch ?? null,
    patchAvailability: item.codePatchAvailability ?? null,
  }))
}
