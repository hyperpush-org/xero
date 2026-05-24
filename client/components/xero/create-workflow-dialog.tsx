import { Copy, Plus, Sparkles, Workflow as WorkflowIcon } from 'lucide-react'

import {
  WORKFLOW_TEMPLATE_LIBRARY,
  type WorkflowTemplateIdDto,
} from '@/src/lib/xero-model/workflow-templates'

import {
  ChoiceCard,
  CreateEntityDialog,
  type CreateEntityDialogView,
} from './create-entity-dialog'

interface CreateWorkflowDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  view: CreateEntityDialogView
  onSetView: (view: CreateEntityDialogView) => void
  canStartBlank: boolean
  canUseAgentCreate?: boolean
  canPickTemplate: boolean
  onStartBlank: () => void
  onUseAgentCreate?: () => void
  onPickTemplate: (templateId: WorkflowTemplateIdDto) => void
}

export function CreateWorkflowDialog({
  open,
  onOpenChange,
  view,
  onSetView,
  canStartBlank,
  canUseAgentCreate = false,
  canPickTemplate,
  onStartBlank,
  onUseAgentCreate,
  onPickTemplate,
}: CreateWorkflowDialogProps) {
  return (
    <CreateEntityDialog
      open={open}
      onOpenChange={onOpenChange}
      view={view}
      onSetView={onSetView}
      title="Create workflow"
      icon={<WorkflowIcon className="h-4 w-4" />}
      choiceDescription="Start from a blank workflow, ask Agent Create for help, or use a starter template."
      templatesDescription="Templates open as editable workflow drafts on the canvas."
      footerNote="Workflows connect agents into reusable pipelines."
      blankChoice={{
        icon: <Plus className="h-4 w-4" />,
        title: 'Blank workflow',
        description: 'Open an empty workflow draft on the canvas.',
        disabled: !canStartBlank,
        onClick: onStartBlank,
      }}
      extraChoices={
        canUseAgentCreate && onUseAgentCreate
          ? [
              {
                icon: <Sparkles className="h-4 w-4" />,
                title: 'Use Agent Create',
                description: 'Describe the workflow and let Agent Create draft it for approval.',
                onClick: onUseAgentCreate,
              },
            ]
          : undefined
      }
      templateChoice={
        canPickTemplate
          ? {
              icon: <Copy className="h-4 w-4" />,
              title: 'From template',
              description: 'Start with a workflow pattern and edit it freely.',
              onClick: () => onSetView('templates'),
            }
          : undefined
      }
      templatesContent={<WorkflowTemplatePicker onPickTemplate={onPickTemplate} />}
    />
  )
}

function WorkflowTemplatePicker({
  onPickTemplate,
}: {
  onPickTemplate: (templateId: WorkflowTemplateIdDto) => void
}) {
  return (
    <div className="flex flex-col gap-2">
      {WORKFLOW_TEMPLATE_LIBRARY.map((template) => (
        <ChoiceCard
          key={template.id}
          icon={<Sparkles className="h-4 w-4" />}
          title={template.name}
          description={`${template.difficulty} - ${template.nodeCount} nodes - ${template.description}`}
          onClick={() => onPickTemplate(template.id)}
        />
      ))}
    </div>
  )
}
