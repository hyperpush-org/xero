import type { Phase } from '@/components/xero/data'
import { z } from 'zod'
import {
  changeKindSchema,
  normalizeOptionalText,
  normalizeText,
  nullableTextSchema,
  payloadBudgetDiagnosticSchema,
  phaseStatusSchema,
  safePercent,
  type PayloadBudgetDiagnosticDto,
} from './shared'
import { providerModelThinkingEffortSchema } from './provider-models'

export const projectSummarySchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  description: z.string(),
  milestone: z.string(),
  totalPhases: z.number().int().nonnegative(),
  completedPhases: z.number().int().nonnegative(),
  activePhase: z.number().int().nonnegative(),
  branch: nullableTextSchema,
  runtime: nullableTextSchema,
})

export const phaseSummarySchema = z.object({
  id: z.number().int().nonnegative(),
  name: z.string().min(1),
  description: z.string(),
  status: phaseStatusSchema,
  currentStep: nullableTextSchema,
  taskCount: z.number().int().nonnegative(),
  completedTasks: z.number().int().nonnegative(),
  summary: nullableTextSchema,
})

export const repositorySummarySchema = z.object({
  id: z.string().min(1),
  projectId: z.string().min(1),
  rootPath: z.string().min(1),
  displayName: z.string().min(1),
  branch: nullableTextSchema,
  headSha: nullableTextSchema,
  isGitRepo: z.boolean(),
})

export const repositoryDiffScopeSchema = z.enum(['staged', 'unstaged', 'worktree'])

const projectTreePathSchema = z
  .string()
  .trim()
  .min(1)
  .refine((value) => value === '/' || value.startsWith('/'), 'Project file paths must start with `/`.')

export const projectEntryKindSchema = z.enum(['file', 'folder'])
export const projectTextRendererKindSchema = z.enum(['code', 'svg', 'markdown', 'csv', 'html'])
export const projectRenderableRendererKindSchema = z.enum(['image', 'pdf', 'audio', 'video'])
export const projectFileRendererKindSchema = z.union([
  projectTextRendererKindSchema,
  projectRenderableRendererKindSchema,
])

export interface ProjectFileNodeDto {
  name: string
  path: string
  type: z.infer<typeof projectEntryKindSchema>
  children?: ProjectFileNodeDto[]
  childrenLoaded?: boolean
  truncated?: boolean
  omittedEntryCount?: number
}

export const projectFileNodeSchema: z.ZodType<ProjectFileNodeDto> = z.lazy(() =>
  z
    .object({
      name: z.string().trim().min(1),
      path: projectTreePathSchema,
      type: projectEntryKindSchema,
      children: z.array(projectFileNodeSchema).optional(),
      childrenLoaded: z.boolean().optional(),
      truncated: z.boolean().optional(),
      omittedEntryCount: z.number().int().nonnegative().optional(),
    })
    .strict(),
)

export const listProjectFilesRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema.default('/'),
  })
  .strict()

export const projectFileRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema,
  })
  .strict()

export const revokeProjectAssetTokensRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    paths: z.array(projectTreePathSchema).default([]),
  })
  .strict()

export const writeProjectFileRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema,
    content: z.string(),
  })
  .strict()

export const createProjectEntryRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    parentPath: projectTreePathSchema,
    name: z.string().trim().min(1),
    entryType: projectEntryKindSchema,
  })
  .strict()

export const renameProjectEntryRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema,
    newName: z.string().trim().min(1),
  })
  .strict()

export const moveProjectEntryRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema,
    targetParentPath: projectTreePathSchema,
  })
  .strict()

export const importRepositoryResponseSchema = z.object({
  project: projectSummarySchema,
  repository: repositorySummarySchema,
})

export const listProjectsResponseSchema = z.object({
  projects: z.array(projectSummarySchema),
})

export const branchSummarySchema = z.object({
  name: z.string().min(1),
  headSha: nullableTextSchema,
  detached: z.boolean(),
  upstream: z
    .object({
      name: z.string().min(1),
      ahead: z.number().int().nonnegative(),
      behind: z.number().int().nonnegative(),
    })
    .nullable()
    .optional(),
})

export const repositoryStatusEntrySchema = z.object({
  path: z.string().min(1),
  staged: changeKindSchema.nullable().optional(),
  unstaged: changeKindSchema.nullable().optional(),
  untracked: z.boolean(),
})

export const repositoryLastCommitSchema = z.object({
  sha: z.string().min(1),
  summary: z.string().min(1),
  committedAt: nullableTextSchema,
})

export const repositoryStatusResponseSchema = z.object({
  repository: repositorySummarySchema,
  branch: branchSummarySchema.nullable().optional(),
  lastCommit: repositoryLastCommitSchema.nullable().optional(),
  entries: z.array(repositoryStatusEntrySchema),
  hasStagedChanges: z.boolean(),
  hasUnstagedChanges: z.boolean(),
  hasUntrackedChanges: z.boolean(),
  additions: z.number().int().nonnegative().optional(),
  deletions: z.number().int().nonnegative().optional(),
  payloadBudget: payloadBudgetDiagnosticSchema.nullable().optional(),
})

export const repositoryDiffResponseSchema = z.object({
  repository: repositorySummarySchema,
  scope: repositoryDiffScopeSchema,
  patch: z.string(),
  truncated: z.boolean(),
  baseRevision: nullableTextSchema,
  payloadBudget: payloadBudgetDiagnosticSchema.nullable().optional(),
})

export const gitPathsRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    paths: z.array(z.string().trim().min(1)).default([]),
  })
  .strict()

export const gitCommitRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    message: z.string().min(1),
  })
  .strict()

export const gitGenerateCommitMessageRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    providerProfileId: z.string().trim().min(1).nullable().optional(),
    modelId: z.string().trim().min(1),
    thinkingEffort: providerModelThinkingEffortSchema.nullable().optional(),
  })
  .strict()

export const gitGenerateCommitMessageResponseSchema = z
  .object({
    message: z.string().trim().min(1),
    providerId: z.string().trim().min(1),
    modelId: z.string().trim().min(1),
    diffTruncated: z.boolean(),
  })
  .strict()

export const gitRemoteRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    remote: z.string().trim().min(1).nullable().optional(),
  })
  .strict()

export const gitSignatureSchema = z.object({
  name: z.string(),
  email: z.string(),
})

export const gitCommitResponseSchema = z.object({
  sha: z.string(),
  summary: z.string(),
  signature: gitSignatureSchema,
})

export const gitFetchResponseSchema = z.object({
  remote: z.string(),
  refspecs: z.array(z.string()),
})

export const gitPullResponseSchema = z.object({
  remote: z.string(),
  branch: z.string(),
  updated: z.boolean(),
  summary: z.string(),
  newHeadSha: nullableTextSchema,
})

export const gitRemoteRefUpdateSchema = z.object({
  refName: z.string(),
  ok: z.boolean(),
  message: nullableTextSchema,
})

export const gitPushResponseSchema = z.object({
  remote: z.string(),
  branch: z.string(),
  updates: z.array(gitRemoteRefUpdateSchema),
})

export const listProjectFilesResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema,
    root: projectFileNodeSchema,
    truncated: z.boolean().optional(),
    omittedEntryCount: z.number().int().nonnegative().optional(),
    payloadBudget: payloadBudgetDiagnosticSchema.nullable().optional(),
  })
  .strict()

const projectFileContentBaseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema,
    byteLength: z.number().int().nonnegative(),
    modifiedAt: z.string().trim().min(1),
    contentHash: z.string().trim().min(1),
  })
  .strict()

export const readProjectFileResponseSchema = z.discriminatedUnion('kind', [
  projectFileContentBaseSchema
    .extend({
      kind: z.literal('text'),
      mimeType: z.string().trim().min(1),
      rendererKind: projectTextRendererKindSchema,
      text: z.string(),
    })
    .strict(),
  projectFileContentBaseSchema
    .extend({
      kind: z.literal('renderable'),
      mimeType: z.string().trim().min(1),
      rendererKind: projectRenderableRendererKindSchema,
      previewUrl: z.string().trim().min(1),
    })
    .strict(),
  projectFileContentBaseSchema
    .extend({
      kind: z.literal('unsupported'),
      mimeType: z.string().trim().min(1).nullable(),
      rendererKind: projectFileRendererKindSchema.nullable().optional(),
      reason: z.string().trim().min(1),
    })
    .strict(),
])

export const writeProjectFileResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema,
  })
  .strict()

export const createProjectEntryResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema,
  })
  .strict()

export const renameProjectEntryResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema,
  })
  .strict()

export const moveProjectEntryResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema,
  })
  .strict()

export const deleteProjectEntryResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema,
  })
  .strict()

export const searchProjectRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    query: z.string().min(1),
    cursor: projectTreePathSchema.optional(),
    caseSensitive: z.boolean().default(false),
    wholeWord: z.boolean().default(false),
    regex: z.boolean().default(false),
    includeGlobs: z.array(z.string().trim().min(1)).default([]),
    excludeGlobs: z.array(z.string().trim().min(1)).default([]),
    maxResults: z.number().int().positive().optional(),
    maxFiles: z.number().int().positive().optional(),
  })
  .strict()

export const searchMatchSchema = z
  .object({
    line: z.number().int().positive(),
    column: z.number().int().positive(),
    previewPrefix: z.string(),
    previewMatch: z.string(),
    previewSuffix: z.string(),
  })
  .strict()

export const searchFileResultSchema = z
  .object({
    path: projectTreePathSchema,
    matches: z.array(searchMatchSchema),
  })
  .strict()

export const searchProjectResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    totalMatches: z.number().int().nonnegative(),
    totalFiles: z.number().int().nonnegative(),
    truncated: z.boolean(),
    nextCursor: projectTreePathSchema.nullable().optional(),
    payloadBudget: payloadBudgetDiagnosticSchema.nullable().optional(),
    files: z.array(searchFileResultSchema),
  })
  .strict()

export const replaceInProjectRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    query: z.string().min(1),
    replacement: z.string(),
    caseSensitive: z.boolean().default(false),
    wholeWord: z.boolean().default(false),
    regex: z.boolean().default(false),
    includeGlobs: z.array(z.string().trim().min(1)).default([]),
    excludeGlobs: z.array(z.string().trim().min(1)).default([]),
    targetPaths: z.array(projectTreePathSchema).optional(),
  })
  .strict()

export const replaceInProjectResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    filesChanged: z.number().int().nonnegative(),
    totalReplacements: z.number().int().nonnegative(),
  })
  .strict()

export const projectUpdatedPayloadSchema = z.object({
  project: projectSummarySchema,
  reason: z.enum(['imported', 'refreshed', 'metadata_changed']),
})

export const repositoryStatusChangedPayloadSchema = z.object({
  projectId: z.string().min(1),
  repositoryId: z.string().min(1),
  status: repositoryStatusResponseSchema,
})

export type ProjectSummaryDto = z.infer<typeof projectSummarySchema>
export type PhaseSummaryDto = z.infer<typeof phaseSummarySchema>
export type RepositorySummaryDto = z.infer<typeof repositorySummarySchema>
export type RepositoryDiffScope = z.infer<typeof repositoryDiffScopeSchema>
export type ImportRepositoryResponseDto = z.infer<typeof importRepositoryResponseSchema>
export type ListProjectsResponseDto = z.infer<typeof listProjectsResponseSchema>
export type RepositoryStatusResponseDto = z.infer<typeof repositoryStatusResponseSchema>
export type RepositoryDiffResponseDto = z.infer<typeof repositoryDiffResponseSchema>
export type ProjectEntryKindDto = z.infer<typeof projectEntryKindSchema>
export type ProjectTextRendererKindDto = z.infer<typeof projectTextRendererKindSchema>
export type ProjectRenderableRendererKindDto = z.infer<typeof projectRenderableRendererKindSchema>
export type ProjectFileRendererKindDto = z.infer<typeof projectFileRendererKindSchema>
export type ListProjectFilesRequestDto = z.infer<typeof listProjectFilesRequestSchema>
export type ProjectFileRequestDto = z.infer<typeof projectFileRequestSchema>
export type RevokeProjectAssetTokensRequestDto = z.infer<typeof revokeProjectAssetTokensRequestSchema>
export type WriteProjectFileRequestDto = z.infer<typeof writeProjectFileRequestSchema>
export type CreateProjectEntryRequestDto = z.infer<typeof createProjectEntryRequestSchema>
export type RenameProjectEntryRequestDto = z.infer<typeof renameProjectEntryRequestSchema>
export type MoveProjectEntryRequestDto = z.infer<typeof moveProjectEntryRequestSchema>
export type ListProjectFilesResponseDto = z.infer<typeof listProjectFilesResponseSchema>
export type ReadProjectFileResponseDto = z.infer<typeof readProjectFileResponseSchema>
export type WriteProjectFileResponseDto = z.infer<typeof writeProjectFileResponseSchema>
export type CreateProjectEntryResponseDto = z.infer<typeof createProjectEntryResponseSchema>
export type RenameProjectEntryResponseDto = z.infer<typeof renameProjectEntryResponseSchema>
export type MoveProjectEntryResponseDto = z.infer<typeof moveProjectEntryResponseSchema>
export type DeleteProjectEntryResponseDto = z.infer<typeof deleteProjectEntryResponseSchema>
export type ProjectUpdatedPayloadDto = z.infer<typeof projectUpdatedPayloadSchema>
export type RepositoryStatusChangedPayloadDto = z.infer<typeof repositoryStatusChangedPayloadSchema>
export type SearchProjectRequestDto = z.infer<typeof searchProjectRequestSchema>
export type SearchMatchDto = z.infer<typeof searchMatchSchema>
export type SearchFileResultDto = z.infer<typeof searchFileResultSchema>
export type SearchProjectResponseDto = z.infer<typeof searchProjectResponseSchema>
export type ReplaceInProjectRequestDto = z.infer<typeof replaceInProjectRequestSchema>
export type ReplaceInProjectResponseDto = z.infer<typeof replaceInProjectResponseSchema>
export type GitPathsRequestDto = z.infer<typeof gitPathsRequestSchema>
export type GitCommitRequestDto = z.infer<typeof gitCommitRequestSchema>
export type GitRemoteRequestDto = z.infer<typeof gitRemoteRequestSchema>
export type GitSignatureDto = z.infer<typeof gitSignatureSchema>
export type GitCommitResponseDto = z.infer<typeof gitCommitResponseSchema>
export type GitGenerateCommitMessageRequestDto = z.infer<typeof gitGenerateCommitMessageRequestSchema>
export type GitGenerateCommitMessageResponseDto = z.infer<typeof gitGenerateCommitMessageResponseSchema>
export type GitFetchResponseDto = z.infer<typeof gitFetchResponseSchema>
export type GitPullResponseDto = z.infer<typeof gitPullResponseSchema>
export type GitRemoteRefUpdateDto = z.infer<typeof gitRemoteRefUpdateSchema>
export type GitPushResponseDto = z.infer<typeof gitPushResponseSchema>

export interface ProjectListItem {
  id: string
  name: string
  description: string
  milestone: string
  totalPhases: number
  completedPhases: number
  activePhase: number
  branch: string
  runtime: string
  branchLabel: string
  runtimeLabel: string
  phaseProgressPercent: number
}

export interface RepositoryView {
  id: string
  projectId: string
  rootPath: string
  displayName: string
  branch: string | null
  branchLabel: string
  headSha: string | null
  headShaLabel: string
  isGitRepo: boolean
}

export interface RepositoryStatusEntryView {
  path: string
  staged: z.infer<typeof changeKindSchema> | null
  unstaged: z.infer<typeof changeKindSchema> | null
  untracked: boolean
}

export interface RepositoryLastCommitView {
  sha: string
  shortShaLabel: string
  summary: string
  committedAt: string | null
}

export interface RepositoryUpstreamView {
  name: string
  ahead: number
  behind: number
}

export interface RepositoryStatusView {
  projectId: string
  repositoryId: string
  diffRevision: string
  branchLabel: string
  headShaLabel: string
  upstream?: RepositoryUpstreamView | null
  lastCommit: RepositoryLastCommitView | null
  stagedCount: number
  unstagedCount: number
  untrackedCount: number
  statusCount: number
  additions: number
  deletions: number
  hasChanges: boolean
  entries: RepositoryStatusEntryView[]
  payloadBudget?: PayloadBudgetDiagnosticDto | null
}

export interface RepositoryDiffView {
  projectId: string
  repositoryId: string
  scope: RepositoryDiffScope
  patch: string
  isEmpty: boolean
  truncated: boolean
  baseRevisionLabel: string
  payloadBudget?: PayloadBudgetDiagnosticDto | null
}

export function mapProjectSummary(dto: ProjectSummaryDto): ProjectListItem {
  const branch = normalizeOptionalText(dto.branch)
  const runtime = normalizeOptionalText(dto.runtime)

  return {
    id: dto.id,
    name: normalizeText(dto.name, 'Untitled project'),
    description: normalizeText(dto.description, 'No description provided.'),
    milestone: normalizeText(dto.milestone, 'No milestone assigned'),
    totalPhases: dto.totalPhases,
    completedPhases: Math.min(dto.completedPhases, dto.totalPhases),
    activePhase: dto.activePhase,
    branch: branch ?? 'No branch',
    runtime: runtime ?? 'Runtime unavailable',
    runtimeLabel: runtime ?? 'Runtime unavailable',
    branchLabel: branch ?? 'No branch',
    phaseProgressPercent: safePercent(dto.completedPhases, dto.totalPhases),
  }
}

export function mapRepository(repository: RepositorySummaryDto): RepositoryView {
  const branch = normalizeOptionalText(repository.branch)
  const headSha = normalizeOptionalText(repository.headSha)

  return {
    id: repository.id,
    projectId: repository.projectId,
    rootPath: repository.rootPath,
    displayName: repository.displayName,
    branch,
    branchLabel: branch ?? 'No branch',
    headSha,
    headShaLabel: headSha ?? 'No HEAD',
    isGitRepo: repository.isGitRepo,
  }
}

export function mapPhase(phase: PhaseSummaryDto): Phase {
  const taskCount = phase.taskCount
  const completedTasks = Math.min(phase.completedTasks, taskCount)

  return {
    id: phase.id,
    name: normalizeText(phase.name, `Phase ${phase.id}`),
    description: normalizeText(phase.description, 'No phase description provided.'),
    status: phase.status,
    currentStep: normalizeOptionalText(phase.currentStep),
    taskCount,
    completedTasks,
    summary: normalizeOptionalText(phase.summary) ?? undefined,
  }
}

export function mapRepositoryStatus(status: RepositoryStatusResponseDto): RepositoryStatusView {
  const branchName = normalizeOptionalText(status.branch?.name) ?? normalizeOptionalText(status.repository.branch)
  const headSha = normalizeOptionalText(status.branch?.headSha) ?? normalizeOptionalText(status.repository.headSha)
  const lastCommitSha = normalizeOptionalText(status.lastCommit?.sha)
  const lastCommitSummary = normalizeOptionalText(status.lastCommit?.summary)
  const lastCommitCommittedAt = normalizeOptionalText(status.lastCommit?.committedAt)
  const entries = status.entries.map((entry) => ({
    path: entry.path,
    staged: entry.staged ?? null,
    unstaged: entry.unstaged ?? null,
    untracked: entry.untracked,
  }))

  const stagedCount = entries.filter((entry) => entry.staged !== null).length
  const unstagedCount = entries.filter((entry) => entry.unstaged !== null).length
  const untrackedCount = entries.filter((entry) => entry.untracked).length
  const uniquePaths = new Set(entries.map((entry) => entry.path))

  const view = {
    projectId: status.repository.projectId,
    repositoryId: status.repository.id,
    branchLabel: branchName ?? 'No branch',
    headShaLabel: headSha ?? 'No HEAD',
    upstream: status.branch?.upstream
      ? {
          name: status.branch.upstream.name,
          ahead: status.branch.upstream.ahead,
          behind: status.branch.upstream.behind,
        }
      : null,
    lastCommit:
      lastCommitSha && lastCommitSummary
        ? {
            sha: lastCommitSha,
            shortShaLabel: lastCommitSha.slice(0, 7),
            summary: lastCommitSummary,
            committedAt: lastCommitCommittedAt,
          }
        : null,
    stagedCount,
    unstagedCount,
    untrackedCount,
    statusCount: uniquePaths.size,
    additions: status.additions ?? 0,
    deletions: status.deletions ?? 0,
    hasChanges:
      status.hasStagedChanges || status.hasUnstagedChanges || status.hasUntrackedChanges || uniquePaths.size > 0,
    entries,
    payloadBudget: status.payloadBudget ?? null,
  }

  return {
    ...view,
    diffRevision: createRepositoryStatusDiffRevision(view),
  }
}

export function mapRepositoryDiff(diff: RepositoryDiffResponseDto): RepositoryDiffView {
  const patch = diff.patch.trim().length > 0 ? diff.patch : ''
  const normalizedBaseRevision = normalizeOptionalText(diff.baseRevision)
  const baseRevisionLabel = normalizedBaseRevision ?? (diff.scope === 'unstaged' ? 'Working tree' : 'No HEAD')

  return {
    projectId: diff.repository.projectId,
    repositoryId: diff.repository.id,
    scope: diff.scope,
    patch,
    isEmpty: patch.length === 0,
    truncated: diff.truncated,
    baseRevisionLabel,
    payloadBudget: diff.payloadBudget ?? null,
  }
}

export function applyProjectSummary<T extends { runtimeSession?: unknown; runtimeRun?: unknown; autonomousRun?: unknown }>(
  project: T & {
    phases: Phase[]
    repository: unknown
    repositoryStatus: unknown
  },
  summary: ProjectListItem,
): T {
  return {
    ...project,
    ...summary,
    phases: project.phases,
    repository: project.repository,
    repositoryStatus: project.repositoryStatus,
    runtimeSession: project.runtimeSession ?? null,
    runtimeRun: project.runtimeRun ?? null,
    autonomousRun: project.autonomousRun ?? null,
  }
}

export function applyRepositoryStatus<T extends { repository: RepositoryView | null; runtimeSession?: unknown; runtimeRun?: unknown }>(
  project: T & { branch: string; branchLabel: string; repositoryStatus: RepositoryStatusView | null },
  status: RepositoryStatusView,
): T {
  const repository = project.repository
    ? {
        ...project.repository,
        branch: status.branchLabel === 'No branch' ? null : status.branchLabel,
        branchLabel: status.branchLabel,
        headSha: status.headShaLabel === 'No HEAD' ? null : status.headShaLabel,
        headShaLabel: status.headShaLabel,
      }
    : project.repository

  return {
    ...project,
    branch: status.branchLabel,
    branchLabel: status.branchLabel,
    repository,
    repositoryStatus: status,
    runtimeSession: project.runtimeSession ?? null,
    runtimeRun: project.runtimeRun ?? null,
  }
}

export function createRepositoryStatusEntriesRevision(entries: RepositoryStatusEntryView[]): string {
  return entries
    .map((entry) =>
      [
        entry.path,
        entry.staged ?? '',
        entry.unstaged ?? '',
        entry.untracked ? '1' : '0',
      ].join('\u0000'),
    )
    .sort()
    .join('\u0001')
}

export function createRepositoryStatusDiffRevision(
  status: Pick<RepositoryStatusView, 'projectId' | 'repositoryId' | 'branchLabel' | 'headShaLabel' | 'entries'> | null,
): string {
  if (!status) {
    return 'none'
  }

  return [
    status.projectId,
    status.repositoryId,
    status.branchLabel,
    status.headShaLabel,
    createRepositoryStatusEntriesRevision(status.entries),
  ].join('\u0002')
}

export function upsertProjectListItem(projects: ProjectListItem[], nextProject: ProjectListItem): ProjectListItem[] {
  const existingIndex = projects.findIndex((project) => project.id === nextProject.id)
  if (existingIndex === -1) {
    return [...projects, nextProject]
  }

  return projects.map((project) => (project.id === nextProject.id ? nextProject : project))
}
