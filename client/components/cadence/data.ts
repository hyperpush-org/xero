export type View = "phases" | "agent" | "execution"

export type PhaseStatus = "complete" | "active" | "pending" | "blocked"
export type PhaseStep = "discuss" | "plan" | "execute" | "verify" | "ship"

export interface Phase {
  id: number
  name: string
  description: string
  status: PhaseStatus
  currentStep: PhaseStep | null
  stepStatuses: Record<PhaseStep, "complete" | "active" | "pending" | "skipped">
  taskCount: number
  completedTasks: number
  waveCount?: number
  completedWaves?: number
  commits?: number
  summary?: string
}

export type ToolType = "read" | "write" | "bash" | "search" | "think" | "spawn"
export type ToolStatus = "pending" | "running" | "complete" | "error"

export interface ToolCall {
  id: string
  type: ToolType
  label: string
  detail: string
  status: ToolStatus
  duration?: string
  output?: string
}

export interface AgentMessage {
  id: string
  role: "user" | "agent" | "system"
  content: string
  timestamp: string
  tools?: ToolCall[]
  phase?: number
  planRef?: string
}

export interface WaveTask {
  id: string
  name: string
  status: "pending" | "running" | "complete" | "failed"
  planFile: string
  commits: number
  tools: number
  duration?: string
}

export interface Wave {
  id: number
  tasks: WaveTask[]
  status: "pending" | "running" | "complete"
  parallel: boolean
}

export interface Project {
  id: string
  name: string
  description: string
  milestone: string
  totalPhases: number
  completedPhases: number
  activePhase: number
  phases: Phase[]
  branch: string
  runtime: string
}

export const MOCK_PROJECTS: Project[] = [
  {
    id: "1",
    name: "api-gateway",
    description: "REST API gateway with JWT auth and rate limiting",
    milestone: "v1.0 — Core Infrastructure",
    totalPhases: 5,
    completedPhases: 2,
    activePhase: 3,
    branch: "feat/phase-3-middleware",
    runtime: "claude-code",
    phases: [
      {
        id: 1,
        name: "Project Setup",
        description: "Initialize repo, configure tooling, establish base structure",
        status: "complete",
        currentStep: null,
        stepStatuses: { discuss: "complete", plan: "complete", execute: "complete", verify: "complete", ship: "complete" },
        taskCount: 3,
        completedTasks: 3,
        waveCount: 2,
        completedWaves: 2,
        commits: 6,
        summary: "Initialized monorepo with Turborepo, configured ESLint/Prettier, set up CI pipeline."
      },
      {
        id: 2,
        name: "Data Models",
        description: "Design and implement core database schema and ORM layer",
        status: "complete",
        currentStep: null,
        stepStatuses: { discuss: "complete", plan: "complete", execute: "complete", verify: "complete", ship: "complete" },
        taskCount: 4,
        completedTasks: 4,
        waveCount: 2,
        completedWaves: 2,
        commits: 8,
        summary: "Created user, API key, rate limit, and audit log models. Schema drift detection enabled."
      },
      {
        id: 3,
        name: "Auth Middleware",
        description: "JWT verification, refresh token rotation, permission guards",
        status: "active",
        currentStep: "execute",
        stepStatuses: { discuss: "complete", plan: "complete", execute: "active", verify: "pending", ship: "pending" },
        taskCount: 4,
        completedTasks: 2,
        waveCount: 3,
        completedWaves: 1,
        commits: 4,
      },
      {
        id: 4,
        name: "Rate Limiting",
        description: "Token bucket algorithm, per-key and per-IP limits, Redis backend",
        status: "pending",
        currentStep: null,
        stepStatuses: { discuss: "pending", plan: "pending", execute: "pending", verify: "pending", ship: "pending" },
        taskCount: 3,
        completedTasks: 0,
      },
      {
        id: 5,
        name: "Observability",
        description: "Structured logging, metrics, distributed tracing, dashboards",
        status: "pending",
        currentStep: null,
        stepStatuses: { discuss: "pending", plan: "pending", execute: "pending", verify: "pending", ship: "pending" },
        taskCount: 4,
        completedTasks: 0,
      },
    ]
  },
  {
    id: "2",
    name: "dashboard-ui",
    description: "Admin control plane with analytics and user management",
    milestone: "v1.0 — MVP Dashboard",
    totalPhases: 4,
    completedPhases: 0,
    activePhase: 1,
    branch: "feat/phase-1-scaffold",
    runtime: "opencode",
    phases: [
      {
        id: 1,
        name: "Scaffold & Routing",
        description: "App shell, navigation, route structure, auth guards",
        status: "active",
        currentStep: "discuss",
        stepStatuses: { discuss: "active", plan: "pending", execute: "pending", verify: "pending", ship: "pending" },
        taskCount: 3,
        completedTasks: 0,
      },
      {
        id: 2,
        name: "Analytics Views",
        description: "Charts, metrics, time-series data visualization",
        status: "pending",
        currentStep: null,
        stepStatuses: { discuss: "pending", plan: "pending", execute: "pending", verify: "pending", ship: "pending" },
        taskCount: 4,
        completedTasks: 0,
      },
      {
        id: 3,
        name: "User Management",
        description: "CRUD operations, role assignment, invitation flow",
        status: "pending",
        currentStep: null,
        stepStatuses: { discuss: "pending", plan: "pending", execute: "pending", verify: "pending", ship: "pending" },
        taskCount: 3,
        completedTasks: 0,
      },
      {
        id: 4,
        name: "Settings & Billing",
        description: "Account settings, subscription management, API key generation",
        status: "pending",
        currentStep: null,
        stepStatuses: { discuss: "pending", plan: "pending", execute: "pending", verify: "pending", ship: "pending" },
        taskCount: 4,
        completedTasks: 0,
      },
    ]
  },
  {
    id: "3",
    name: "data-pipeline",
    description: "ETL pipeline for event ingestion and analytics aggregation",
    milestone: "v0.1 — Proof of Concept",
    totalPhases: 3,
    completedPhases: 0,
    activePhase: 1,
    branch: "main",
    runtime: "cursor",
    phases: [
      {
        id: 1,
        name: "Requirements",
        description: "Scope data sources, throughput targets, storage strategy",
        status: "active",
        currentStep: "discuss",
        stepStatuses: { discuss: "active", plan: "pending", execute: "pending", verify: "pending", ship: "pending" },
        taskCount: 2,
        completedTasks: 0,
      },
      {
        id: 2,
        name: "Ingestion Layer",
        description: "Kafka consumers, schema validation, dead letter queues",
        status: "pending",
        currentStep: null,
        stepStatuses: { discuss: "pending", plan: "pending", execute: "pending", verify: "pending", ship: "pending" },
        taskCount: 4,
        completedTasks: 0,
      },
      {
        id: 3,
        name: "Aggregation & Storage",
        description: "Stream processing, materialized views, ClickHouse integration",
        status: "pending",
        currentStep: null,
        stepStatuses: { discuss: "pending", plan: "pending", execute: "pending", verify: "pending", ship: "pending" },
        taskCount: 5,
        completedTasks: 0,
      },
    ]
  }
]

export const MOCK_MESSAGES: AgentMessage[] = [
  {
    id: "sys-1",
    role: "system",
    content: "Session started · Phase 3 · Auth Middleware · Wave 2 of 3",
    timestamp: "10:31 AM",
  },
  {
    id: "1",
    role: "user",
    content: "Implement the JWT authentication middleware. Use RS256 signing with refresh token rotation — 15min access, 7d refresh.",
    timestamp: "10:32 AM",
    phase: 3,
    planRef: "3-02-PLAN.md"
  },
  {
    id: "2",
    role: "agent",
    content: "Starting execution of Plan 3-02. Reading existing middleware structure, then implementing JWT verification with RS256 and the refresh rotation logic.",
    timestamp: "10:32 AM",
    tools: [
      { id: "t1", type: "read", label: "Read", detail: "src/middleware/index.ts", status: "complete", duration: "0.1s" },
      { id: "t2", type: "read", label: "Read", detail: "src/types/auth.ts", status: "complete", duration: "0.1s" },
      { id: "t3", type: "search", label: "Search", detail: "existing token validation patterns", status: "complete", duration: "0.3s", output: "Found 2 references in auth.service.ts" },
      { id: "t4", type: "think", label: "Think", detail: "Planning RS256 key loading strategy — file vs env vs KMS", status: "complete", duration: "1.2s" },
      { id: "t5", type: "write", label: "Write", detail: "src/middleware/auth.ts (142 lines)", status: "complete", duration: "2.1s" },
      { id: "t6", type: "write", label: "Write", detail: "src/middleware/refresh.ts (89 lines)", status: "complete", duration: "1.4s" },
      { id: "t7", type: "bash", label: "Bash", detail: "git commit -m 'feat(auth): JWT RS256 middleware with refresh rotation'", status: "complete", duration: "0.4s", output: "[feat/phase-3-middleware a3f91b2] feat(auth): JWT RS256 middleware with refresh rotation\n 2 files changed, 231 insertions(+)" },
    ]
  },
  {
    id: "3",
    role: "agent",
    content: "Plan 3-02 complete. Committed `auth.ts` and `refresh.ts`. Moving to permission guards (Plan 3-03).",
    timestamp: "10:38 AM",
    tools: [
      { id: "t8", type: "spawn", label: "Spawn", detail: "Subagent: plan-checker verifying 3-03-PLAN.md", status: "running", },
    ]
  },
]

export const MOCK_WAVES: Wave[] = [
  {
    id: 1,
    status: "complete",
    parallel: false,
    tasks: [
      { id: "w1t1", name: "Token types & interfaces", status: "complete", planFile: "3-01-PLAN.md", commits: 2, tools: 8, duration: "3m 14s" },
    ]
  },
  {
    id: 2,
    status: "running",
    parallel: true,
    tasks: [
      { id: "w2t1", name: "JWT RS256 middleware", status: "complete", planFile: "3-02-PLAN.md", commits: 2, tools: 7, duration: "6m 02s" },
      { id: "w2t2", name: "Permission guards", status: "running", planFile: "3-03-PLAN.md", commits: 0, tools: 3 },
    ]
  },
  {
    id: 3,
    status: "pending",
    parallel: false,
    tasks: [
      { id: "w3t1", name: "Integration tests + verify", status: "pending", planFile: "3-04-PLAN.md", commits: 0, tools: 0 },
    ]
  },
]
