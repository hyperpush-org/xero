import { ArrowRight, FileWarning, Settings, Sparkles } from "lucide-react"

import type {
  RuntimeStreamToolItemView,
  RuntimeStreamViewItem,
} from "@/src/lib/xero-model"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { cn } from "@/lib/utils"

const AGENT_DEFINITION_TOOL_NAME = "agent_definition"

interface AgentCreateDraftSectionProps {
  runtimeStreamItems: readonly RuntimeStreamViewItem[]
  pendingApprovalCount: number
  onOpenAgentManagement?: () => void
}

function isAgentDefinitionToolItem(item: RuntimeStreamViewItem): item is RuntimeStreamToolItemView {
  return item.kind === "tool" && item.toolName === AGENT_DEFINITION_TOOL_NAME
}

export function AgentCreateDraftSection({
  runtimeStreamItems,
  pendingApprovalCount,
  onOpenAgentManagement,
}: AgentCreateDraftSectionProps) {
  const recentDraftItems = runtimeStreamItems
    .filter(isAgentDefinitionToolItem)
    .slice(-4)
    .reverse()

  if (recentDraftItems.length === 0) {
    return (
      <Card className="border-border/60 bg-primary/[0.04]">
        <CardHeader className="pb-2">
          <CardTitle className="flex items-center gap-2 text-[13px] font-semibold">
            <Sparkles className="h-4 w-4 text-primary" aria-hidden="true" />
            Agent Create
          </CardTitle>
          <CardDescription className="text-[12px]">
            Describe the agent you want and Agent Create will draft a definition. Saving requires
            explicit approval through the action panel below.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex items-center justify-between text-[12px] text-muted-foreground">
          <span>Activated drafts appear in the agent selector and can be archived from settings.</span>
          {onOpenAgentManagement ? (
            <Button
              variant="ghost"
              size="sm"
              className="h-7 gap-1.5 text-[11.5px]"
              onClick={onOpenAgentManagement}
            >
              <Settings className="h-3 w-3" />
              Manage agents
              <ArrowRight className="h-3 w-3" />
            </Button>
          ) : null}
        </CardContent>
      </Card>
    )
  }

  return (
    <Card className="border-border/60">
      <CardHeader className="pb-2">
        <CardTitle className="flex items-center gap-2 text-[13px] font-semibold">
          <Sparkles className="h-4 w-4 text-primary" aria-hidden="true" />
          Agent Create draft activity
          {pendingApprovalCount > 0 ? (
            <Badge variant="secondary" className="ml-1 text-[10px]">
              {pendingApprovalCount} pending approval{pendingApprovalCount === 1 ? "" : "s"}
            </Badge>
          ) : null}
        </CardTitle>
        <CardDescription className="text-[12px]">
          Recent definition tool calls from this run. Approve a save below to activate the
          custom agent.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        {recentDraftItems.map((item) => (
          <DraftRow key={item.id} item={item} />
        ))}
        {onOpenAgentManagement ? (
          <Button
            variant="ghost"
            size="sm"
            className="ml-auto h-7 gap-1.5 text-[11.5px]"
            onClick={onOpenAgentManagement}
          >
            <Settings className="h-3 w-3" />
            Manage agents
            <ArrowRight className="h-3 w-3" />
          </Button>
        ) : null}
      </CardContent>
    </Card>
  )
}

interface DraftRowProps {
  item: RuntimeStreamToolItemView
}

function DraftRow({ item }: DraftRowProps) {
  const isFailed = item.toolState === "failed"
  return (
    <div
      className={cn(
        "rounded-md border border-border/60 bg-card/40 px-3 py-2 text-[12px]",
        isFailed ? "border-destructive/40 bg-destructive/5" : null,
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-1.5 font-medium text-foreground">
          {isFailed ? (
            <FileWarning className="h-3.5 w-3.5 text-destructive" aria-hidden="true" />
          ) : (
            <Sparkles className="h-3.5 w-3.5 text-primary" aria-hidden="true" />
          )}
          agent_definition
        </span>
        <span className="text-[10.5px] uppercase tracking-wider text-muted-foreground">
          {item.toolState}
        </span>
      </div>
      {item.detail ? (
        <p className="mt-1 line-clamp-3 text-muted-foreground">{item.detail}</p>
      ) : null}
    </div>
  )
}
