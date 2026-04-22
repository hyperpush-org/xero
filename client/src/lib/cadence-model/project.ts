import type { Phase, PhaseStatus, PhaseStep } from '@/components/cadence/data'
import { z } from 'zod'
import {
  PHASE_STEPS,
  STEP_INDEX,
  changeKindSchema,
  isoTimestampSchema,
  nonEmptyOptionalTextSchema,
  normalizeOptionalText,
  normalizeText,
  nullableTextSchema,
  phaseStatusSchema,
  phaseStepSchema,
  safePercent,
} from './shared'

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
  currentStep: phaseStepSchema.nullable().optional(),
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

export interface ProjectFileNodeDto {
  name: string
  path: string
  type: z.infer<typeof projectEntryKindSchema>
  children?: ProjectFileNodeDto[]
}

export const projectFileNodeSchema: z.ZodType<ProjectFileNodeDto> = z.lazy(() =>
  z
    .object({
      name: z.string().trim().min(1),
      path: projectTreePathSchema,
      type: projectEntryKindSchema,
      children: z.array(projectFileNodeSchema).optional(),
    })
    .strict(),
)

export const projectFileRequestSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema,
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
})

export const repositoryDiffResponseSchema = z.object({
  repository: repositorySummarySchema,
  scope: repositoryDiffScopeSchema,
  patch: z.string(),
  truncated: z.boolean(),
  baseRevision: nullableTextSchema,
})

export const listProjectFilesResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    root: projectFileNodeSchema,
  })
  .strict()

export const readProjectFileResponseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    path: projectTreePathSchema,
    content: z.string(),
  })
  .strict()

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
    caseSensitive: z.boolean().default(false),
    wholeWord: z.boolean().default(false),
    regex: z.boolean().default(false),
    includeGlobs: z.array(z.string().trim().min(1)).default([]),
    excludeGlobs: z.array(z.string().trim().min(1)).default([]),
    maxResults: z.number().int().positive().optional(),
  })
  .strict()

export const searchMatchSchema = z
  .object({
    line: z.number().int().positive(),
    column: z.number().int().positive(),
    matchStart: z.number().int().nonnegative(),
    matchEnd: z.number().int().nonnegative(),
    preview: z.string(),
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
export type ProjectFileRequestDto = z.infer<typeof projectFileRequestSchema>
export type WriteProjectFileRequestDto = z.infer<typeof writeProjectFileRequestSchema>
export type CreateProjectEntryRequestDto = z.infer<typeof createProjectEntryRequestSchema>
export type RenameProjectEntryRequestDto = z.infer<typeof renameProjectEntryRequestSchema>
export type ListProjectFilesResponseDto = z.infer<typeof listProjectFilesResponseSchema>
export type ReadProjectFileResponseDto = z.infer<typeof readProjectFileResponseSchema>
export type WriteProjectFileResponseDto = z.infer<typeof writeProjectFileResponseSchema>
export type CreateProjectEntryResponseDto = z.infer<typeof createProjectEntryResponseSchema>
export type RenameProjectEntryResponseDto = z.infer<typeof renameProjectEntryResponseSchema>
export type DeleteProjectEntryResponseDto = z.infer<typeof deleteProjectEntryResponseSchema>
export type ProjectUpdatedPayloadDto = z.infer<typeof projectUpdatedPayloadSchema>
export type RepositoryStatusChangedPayloadDto = z.infer<typeof repositoryStatusChangedPayloadSchema>
export type SearchProjectRequestDto = z.infer<typeof searchProjectRequestSchema>
export type SearchMatchDto = z.infer<typeof searchMatchSchema>
export type SearchFileResultDto = z.infer<typeof searchFileResultSchema>
export type SearchProjectResponseDto = z.infer<typeof searchProjectResponseSchema>
export type ReplaceInProjectRequestDto = z.infer<typeof replaceInProjectRequestSchema>
export type ReplaceInProjectResponseDto = z.infer<typeof replaceInProjectResponseSchema>

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

export interface RepositoryStatusView {
  projectId: string
  repositoryId: string
  branchLabel: string
  headShaLabel: string
  lastCommit: RepositoryLastCommitView | null
  stagedCount: number
  unstagedCount: number
  untrackedCount: number
  statusCount: number
  hasChanges: boolean
  entries: RepositoryStatusEntryView[]
}

export interface RepositoryDiffView {
  projectId: string
  repositoryId: string
  scope: RepositoryDiffScope
  patch: string
  isEmpty: boolean
  truncated: boolean
  baseRevisionLabel: string
}

function createStepStatuses(
  status: PhaseStatus,
  currentStep: PhaseStep | null,
): Record<PhaseStep, 'complete' | 'active' | 'pending' | 'skipped'> {
  if (status === 'complete') {
    return {
      discuss: 'complete',
      plan: 'complete',
      execute: 'complete',
      verify: 'complete',
      ship: 'complete',
    }
  }

  if (!currentStep) {
    return {
      discuss: 'pending',
      plan: 'pending',
      execute: 'pending',
      verify: 'pending',
      ship: 'pending',
    }
  }

  const activeIndex = STEP_INDEX.get(currentStep) ?? 0

  return PHASE_STEPS.reduce<Record<PhaseStep, 'complete' | 'active' | 'pending' | 'skipped'>>(
    (acc, step, index) => {
      if (index < activeIndex) {
        acc[step] = 'complete'
      } else if (index === activeIndex) {
        acc[step] = 'active'
      } else {
        acc[step] = 'pending'
      }

      return acc
    },
    {
      discuss: 'pending',
      plan: 'pending',
      execute: 'pending',
      verify: 'pending',
      ship: 'pending',
    },
  )
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
    currentStep: phase.currentStep ?? null,
    stepStatuses: createStepStatuses(phase.status, phase.currentStep ?? null),
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

  return {
    projectId: status.repository.projectId,
    repositoryId: status.repository.id,
    branchLabel: branchName ?? 'No branch',
    headShaLabel: headSha ?? 'No HEAD',
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
    hasChanges:
      status.hasStagedChanges || status.hasUnstagedChanges || status.hasUntrackedChanges || uniquePaths.size > 0,
    entries,
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
  }
}

export function applyProjectSummary<T extends { runtimeSession?: unknown; runtimeRun?: unknown; autonomousRun?: unknown; autonomousUnit?: unknown }>(
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
    autonomousUnit: project.autonomousUnit ?? null,
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

export function upsertProjectListItem(projects: ProjectListItem[], nextProject: ProjectListItem): ProjectListItem[] {
  const existingIndex = projects.findIndex((project) => project.id === nextProject.id)
  if (existingIndex === -1) {
    return [...projects, nextProject]
  }

  return projects.map((project) => (project.id === nextProject.id ? nextProject : project))
}
