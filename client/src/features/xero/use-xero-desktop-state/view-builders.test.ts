import { describe, expect, it } from 'vitest'
import {
  createRepositoryStatusDiffRevision,
  type Phase,
  type ProjectDetailView,
  type RepositoryStatusView,
} from '@/src/lib/xero-model'
import { buildExecutionView } from './view-builders'

function makePhase(overrides: Partial<Phase> = {}): Phase {
  return {
    id: 2,
    name: 'Live projection',
    description: 'Project workflow truth into the shell',
    status: 'active',
    currentStep: 'verify',
    taskCount: 3,
    completedTasks: 2,
    summary: null,
    ...overrides,
  } as Phase
}

function makeProject(overrides: Partial<ProjectDetailView> = {}): ProjectDetailView {
  const phase = makePhase()

  return {
    id: 'project-1',
    name: 'Xero',
    description: 'Desktop shell',
    milestone: 'M010',
    branch: 'main',
    branchLabel: 'main',
    runtimeLabel: 'Openai Codex · Authenticated',
    phaseProgressPercent: 67,
    phases: [phase],
    activePhase: phase.id,
    repository: {
      id: 'repo-project-1',
      projectId: 'project-1',
      rootPath: '/tmp/xero',
      displayName: 'Xero',
      branch: 'main',
      branchLabel: 'main',
      headSha: 'abc1234',
      headShaLabel: 'abc1234',
      isGitRepo: true,
    },
    approvalRequests: [],
    pendingApprovalCount: 0,
    latestDecisionOutcome: null,
    verificationRecords: [],
    resumeHistory: [],
    agentSessions: [],
    selectedAgentSession: null,
    selectedAgentSessionId: 'agent-session-main',
    notificationBroker: {
      projectId: 'project-1',
      dispatches: [],
      actions: [],
      latestActionAt: null,
      pendingActionCount: 0,
      failedActionCount: 0,
    },
    autonomousRun: null,
    ...overrides,
  } as ProjectDetailView
}

function makeRepositoryStatus(overrides: Partial<RepositoryStatusView> = {}): RepositoryStatusView {
  const { diffRevision, ...statusOverrides } = overrides
  const status = {
    projectId: 'project-1',
    repositoryId: 'repo-project-1',
    branchLabel: 'feature/xero',
    headShaLabel: 'def5678',
    upstream: null,
    lastCommit: null,
    entries: [
      {
        path: 'client/src/App.tsx',
        staged: 'modified',
        unstaged: null,
        untracked: false,
      },
    ],
    stagedCount: 1,
    unstagedCount: 2,
    untrackedCount: 0,
    statusCount: 3,
    additions: 4,
    deletions: 1,
    hasChanges: true,
    ...statusOverrides,
  }

  return {
    ...status,
    diffRevision: diffRevision ?? createRepositoryStatusDiffRevision(status),
  }
}

describe('view builders', () => {
  it('buildExecutionView keeps diff-scope counts and durable verification copy aligned with repository truth', () => {
    const project = makeProject({
      verificationRecords: [
        {
          id: 1,
          sourceActionId: 'flow-1:review_worktree',
          status: 'passed',
          statusLabel: 'Passed',
          summary: 'Approved operator action.',
          detail: null,
          recordedAt: '2026-04-20T12:10:00Z',
        },
      ],
      resumeHistory: [
        {
          id: 1,
          sourceActionId: 'flow-1:review_worktree',
          sessionId: 'session-1',
          status: 'started',
          statusLabel: 'Started',
          summary: 'Operator resumed the selected project runtime session.',
          createdAt: '2026-04-20T12:11:00Z',
        },
      ],
    })

    const view = buildExecutionView({
      project,
      activePhase: project.phases[0] ?? null,
      repositoryStatus: makeRepositoryStatus(),
      operatorActionError: null,
    })

    expect(view).toMatchObject({
      branchLabel: 'feature/xero',
      headShaLabel: 'def5678',
      statusCount: 3,
      hasChanges: true,
      verificationUnavailableReason:
        'Durable operator verification and resume history are loaded from the selected project snapshot.',
    })
    expect(view?.diffScopes).toEqual([
      { scope: 'staged', label: 'Staged', count: 1 },
      { scope: 'unstaged', label: 'Unstaged', count: 2 },
      { scope: 'worktree', label: 'Worktree', count: 3 },
    ])
  })
})
