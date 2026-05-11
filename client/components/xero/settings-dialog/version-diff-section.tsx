import { useMemo } from "react"
import { ArrowRight, Loader2 } from "lucide-react"

import type { AgentDefinitionVersionDiffDto } from "@/src/lib/xero-model/agent-definition"
import { cn } from "@/lib/utils"

interface VersionDiffSectionProps {
  status: "idle" | "loading" | "ready" | "error"
  errorMessage: string | null
  diff: AgentDefinitionVersionDiffDto | null
  fromVersion: number | null
  toVersion: number | null
}

const SECTION_LABELS: Record<string, string> = {
  identity: "Identity",
  prompts: "Prompts",
  attachedSkills: "Attached skills",
  toolPolicy: "Tool policy",
  memoryPolicy: "Memory policy",
  retrievalPolicy: "Retrieval policy",
  handoffPolicy: "Handoff policy",
  outputContract: "Output contract",
  databaseAccess: "Database touchpoints",
  consumedArtifacts: "Consumed artifacts",
  workflowStructure: "Workflow",
  safetyLimits: "Safety limits",
}

function sectionLabel(section: string): string {
  return SECTION_LABELS[section] ?? section
}

function formatTimestamp(value: string): string {
  const parsed = Date.parse(value)
  if (!Number.isFinite(parsed)) {
    return value
  }
  return new Date(parsed).toLocaleString()
}

function formatJsonValue(value: unknown): string {
  if (value === null || value === undefined) {
    return "—"
  }
  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}

export function VersionDiffSection({
  status,
  errorMessage,
  diff,
  fromVersion,
  toVersion,
}: VersionDiffSectionProps) {
  const orderedSections = useMemo(() => {
    if (!diff) return []
    const order = Object.keys(SECTION_LABELS)
    return [...diff.sections].sort((a, b) => {
      const ai = order.indexOf(a.section)
      const bi = order.indexOf(b.section)
      if (ai === -1 && bi === -1) return a.section.localeCompare(b.section)
      if (ai === -1) return 1
      if (bi === -1) return -1
      return ai - bi
    })
  }, [diff])

  return (
    <section
      className="rounded-md border border-border/40 bg-background/40"
      aria-label="Version diff"
    >
      <header className="flex items-center justify-between gap-3 border-b border-border/40 px-3 py-2">
        <div className="flex min-w-0 items-center gap-2 text-[12px]">
          <span className="font-medium text-foreground">Diff</span>
          {fromVersion !== null && toVersion !== null ? (
            <span className="flex items-center gap-1 text-muted-foreground">
              v{fromVersion}
              <ArrowRight className="h-3 w-3" aria-hidden="true" />
              v{toVersion}
            </span>
          ) : null}
        </div>
        {diff ? (
          <span className="text-[11px] text-muted-foreground">
            {diff.changedSections.length === 0
              ? "No changes"
              : `${diff.changedSections.length} section${diff.changedSections.length === 1 ? "" : "s"} changed`}
          </span>
        ) : null}
      </header>

      <div className="px-3 py-2.5">
        {status === "loading" ? (
          <div className="flex items-center gap-2 text-[12px] text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            Loading diff
          </div>
        ) : status === "error" ? (
          <p className="text-[12px] text-destructive">
            {errorMessage ?? "Could not load the diff."}
          </p>
        ) : !diff ? (
          <p className="text-[12px] text-muted-foreground">
            Pick two versions above to compare them.
          </p>
        ) : (
          <div className="flex flex-col gap-3">
            <p className="text-[11px] text-muted-foreground">
              Compared snapshots from {formatTimestamp(diff.fromCreatedAt)} and{" "}
              {formatTimestamp(diff.toCreatedAt)}.
            </p>
            {!diff.changed ? (
              <p className="text-[12px] text-muted-foreground">
                These two versions are byte-equivalent across every diff section.
              </p>
            ) : (
              <ul className="flex flex-col gap-2">
                {orderedSections.map((section) => (
                  <li
                    key={section.section}
                    className={cn(
                      "rounded-md border px-2.5 py-2",
                      section.changed
                        ? "border-warning/30 bg-warning/[0.05]"
                        : "border-border/40 bg-secondary/10",
                    )}
                  >
                    <div className="flex items-center justify-between gap-3">
                      <span className="text-[12px] font-medium text-foreground">
                        {sectionLabel(section.section)}
                      </span>
                      <span className="text-[11px] text-muted-foreground">
                        {section.changed ? "changed" : "unchanged"}
                      </span>
                    </div>
                    {section.changed ? (
                      <DiffFieldList
                        fields={section.fields}
                        before={section.before}
                        after={section.after}
                      />
                    ) : null}
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </div>
    </section>
  )
}

interface DiffFieldListProps {
  fields: string[]
  before: Record<string, unknown>
  after: Record<string, unknown>
}

function DiffFieldList({ fields, before, after }: DiffFieldListProps) {
  const changedFields = fields.filter(
    (field) => formatJsonValue(before[field]) !== formatJsonValue(after[field]),
  )
  const visibleFields = changedFields.length > 0 ? changedFields : fields

  return (
    <div className="mt-2 flex flex-col gap-2">
      {visibleFields.map((field) => (
        <div key={field} className="flex flex-col gap-1">
          <span className="font-mono text-[10.5px] uppercase tracking-wide text-muted-foreground">
            {field}
          </span>
          <div className="grid gap-1.5 sm:grid-cols-2">
            <DiffPane tone="before" value={before[field]} />
            <DiffPane tone="after" value={after[field]} />
          </div>
        </div>
      ))}
    </div>
  )
}

interface DiffPaneProps {
  tone: "before" | "after"
  value: unknown
}

function DiffPane({ tone, value }: DiffPaneProps) {
  return (
    <div
      className={cn(
        "rounded-sm border px-2 py-1.5",
        tone === "before"
          ? "border-destructive/20 bg-destructive/[0.04]"
          : "border-success/20 bg-success/[0.04]",
      )}
    >
      <span
        className={cn(
          "block text-[10px] font-semibold uppercase tracking-wide",
          tone === "before" ? "text-destructive" : "text-success",
        )}
      >
        {tone === "before" ? "Before" : "After"}
      </span>
      <pre className="mt-0.5 max-h-48 overflow-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-[1.45] text-foreground/80">
        {formatJsonValue(value)}
      </pre>
    </div>
  )
}
