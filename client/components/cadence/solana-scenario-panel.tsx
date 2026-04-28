"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { ChevronRight, Loader2, PlayCircle } from "lucide-react"
import { cn } from "@/lib/utils"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type {
  ClusterKind,
  Persona,
  ScenarioDescriptor,
  ScenarioRun,
  ScenarioSpec,
} from "@/src/features/solana/use-solana-workbench"

interface SolanaScenarioPanelProps {
  cluster: ClusterKind
  personas: Persona[]
  scenarios: ScenarioDescriptor[]
  busy: boolean
  lastRun: ScenarioRun | null
  clusterRunning: boolean
  onRunScenario: (spec: ScenarioSpec) => Promise<ScenarioRun | null>
}

export function SolanaScenarioPanel({
  cluster,
  personas,
  scenarios,
  busy,
  lastRun,
  clusterRunning,
  onRunScenario,
}: SolanaScenarioPanelProps) {
  const applicableScenarios = useMemo(
    () => scenarios.filter((s) => s.supportedClusters.includes(cluster)),
    [scenarios, cluster],
  )

  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [selectedPersona, setSelectedPersona] = useState<string | null>(null)

  useEffect(() => {
    // Default selection: first applicable scenario.
    if (!selectedId || !applicableScenarios.some((s) => s.id === selectedId)) {
      setSelectedId(applicableScenarios[0]?.id ?? null)
    }
  }, [applicableScenarios, selectedId])

  useEffect(() => {
    // Default persona: first persona whose role matches the scenario's
    // required roles, falling back to the first persona.
    const scenario = applicableScenarios.find((s) => s.id === selectedId)
    if (!scenario) {
      setSelectedPersona(null)
      return
    }
    const matching =
      personas.find((p) => scenario.requiredRoles.includes(p.role)) ?? personas[0]
    setSelectedPersona(matching?.name ?? null)
  }, [applicableScenarios, selectedId, personas])

  const selectedScenario = useMemo(
    () => applicableScenarios.find((s) => s.id === selectedId) ?? null,
    [applicableScenarios, selectedId],
  )

  const handleRun = useCallback(async () => {
    if (!selectedScenario || !selectedPersona) return
    await onRunScenario({
      id: selectedScenario.id,
      cluster,
      persona: selectedPersona,
      params: {},
    })
  }, [selectedScenario, selectedPersona, onRunScenario, cluster])

  return (
    <div className="flex flex-col gap-4">
      {applicableScenarios.length === 0 ? (
        <p className="text-[11.5px] text-muted-foreground">
          No scenarios available on {cluster}. Switch clusters to see available runbooks.
        </p>
      ) : (
        <div className="flex flex-col">
          {applicableScenarios.map((scenario) => {
            const selected = scenario.id === selectedId
            const kindLabel =
              scenario.kind === "self_contained" ? "runs now" : "needs TxPipeline"
            return (
              <button
                key={scenario.id}
                type="button"
                onClick={() => setSelectedId(scenario.id)}
                className={cn(
                  "group flex w-full flex-col items-start gap-1 rounded-md px-2 py-2 text-left transition-colors",
                  selected
                    ? "bg-primary/10 text-primary"
                    : "hover:bg-muted/25",
                )}
              >
                <div className="flex w-full items-center gap-2">
                  <ChevronRight
                    className={cn(
                      "h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform",
                      selected && "rotate-90 text-primary",
                    )}
                  />
                  <span className="flex-1 truncate text-[12.5px] font-medium text-foreground">
                    {scenario.label}
                  </span>
                  <span
                    className={cn(
                      "shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium",
                      scenario.kind === "self_contained"
                        ? "bg-emerald-500/15 text-emerald-400"
                        : "bg-amber-500/15 text-amber-400",
                    )}
                  >
                    {kindLabel}
                  </span>
                </div>
                <p className="pl-5 pr-1 text-[11px] leading-snug text-muted-foreground">
                  {scenario.description}
                </p>
              </button>
            )
          })}
        </div>
      )}

      {selectedScenario ? (
        <div>
          <div className="mb-2 text-[11.5px] font-medium text-foreground">
            Launch
            <span className="ml-1.5 font-normal text-muted-foreground">
              {selectedScenario.label}
            </span>
          </div>
          <div className="flex flex-col gap-1.5">
            <Select
              disabled={personas.length === 0}
              onValueChange={setSelectedPersona}
              value={selectedPersona ?? ""}
            >
              <SelectTrigger
                aria-label="Persona"
                className="h-8 w-full border-border/60 bg-background text-[12px] focus:border-primary/60"
                size="sm"
              >
                <SelectValue placeholder="No personas on this cluster" />
              </SelectTrigger>
              <SelectContent>
                {personas.map((persona) => (
                  <SelectItem key={persona.name} value={persona.name}>
                    {persona.name} · {persona.role}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {selectedScenario.requiredClonePrograms.length > 0 ? (
              <div className="text-[11px] text-muted-foreground">
                Clone programs:{" "}
                {selectedScenario.requiredClonePrograms
                  .map((p) => p.slice(0, 4) + "…" + p.slice(-4))
                  .join(", ")}
              </div>
            ) : null}
            <button
              type="button"
              onClick={handleRun}
              disabled={!selectedPersona || busy || !clusterRunning}
              className={cn(
                "mt-0.5 inline-flex h-8 items-center justify-center gap-1.5 rounded-md bg-primary px-3 text-[12px] font-medium text-primary-foreground transition-colors",
                "hover:bg-primary/90 disabled:opacity-50",
              )}
            >
              {busy ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <PlayCircle className="h-3.5 w-3.5" />
              )}
              Run scenario
            </button>
          </div>
        </div>
      ) : null}

      {lastRun ? (
        <div className="border-t border-border/50 pt-3">
          <div className="flex items-center justify-between gap-2">
            <div className="min-w-0 text-[11px] text-muted-foreground">
              Last run ·{" "}
              <span className="font-mono text-foreground/80">{lastRun.id}</span>
            </div>
            <span
              className={cn(
                "shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium",
                statusColor(lastRun.status),
              )}
            >
              {lastRun.status}
            </span>
          </div>
          {lastRun.pipelineHint ? (
            <p className="mt-1.5 text-[11px] text-amber-400/90">{lastRun.pipelineHint}</p>
          ) : null}
          <div className="mt-1.5 flex flex-col gap-0.5 text-[11px] text-foreground/80">
            {lastRun.steps.map((step, idx) => (
              <span key={idx}>· {step}</span>
            ))}
          </div>
          {lastRun.signatures.length > 0 ? (
            <div className="mt-1 text-[10.5px] text-muted-foreground">
              {lastRun.signatures.length} signature(s) collected
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

function statusColor(status: ScenarioRun["status"]): string {
  switch (status) {
    case "succeeded":
      return "bg-emerald-500/15 text-emerald-400"
    case "failed":
      return "bg-destructive/15 text-destructive"
    case "pendingPipeline":
      return "bg-amber-500/15 text-amber-400"
  }
}
