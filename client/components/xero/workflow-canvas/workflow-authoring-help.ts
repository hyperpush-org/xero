import type {
  WorkflowConditionDto,
  WorkflowDefinitionDto,
  WorkflowEdgeDto,
  WorkflowEdgeTypeDto,
  WorkflowInputBindingDto,
  WorkflowNodeDto,
  WorkflowOutputExtractionDto,
  WorkflowStateQueryDto,
  WorkflowStateQueryFilterDto,
  WorkflowStateWriteOperationDto,
} from '@/src/lib/xero-model/workflow-definition'

export interface WorkflowNodeHelp {
  label: string
  oneLine: string
  useWhen: string
  setup: string[]
  advanced: string[]
}

export interface WorkflowEdgeHelp {
  label: string
  oneLine: string
  useWhen: string
}

export interface WorkflowHandoffPreset {
  artifactType: string
  label: string
  description: string
  extraction: WorkflowOutputExtractionDto
}

export const WORKFLOW_NODE_HELP: Record<WorkflowNodeDto['type'], WorkflowNodeHelp> = {
  agent: {
    label: 'Agent',
    oneLine: 'Runs one agent and saves the result as a handoff for later nodes.',
    useWhen: 'Use this for work that needs judgment, planning, implementation, review, or summarization.',
    setup: ['Choose the agent.', 'Name the handoff it produces.', 'Connect the next path.'],
    advanced: ['Input bindings', 'JSON extraction', 'run overrides', 'resource scopes'],
  },
  router: {
    label: 'Route',
    oneLine: 'Chooses which outgoing path should run next.',
    useWhen: 'Use this after a typed handoff or state query when the Workflow needs an if/else decision.',
    setup: ['Connect each possible path.', 'Pick a condition on each connection.', 'Leave one default path when useful.'],
    advanced: ['Condition JSON paths', 'edge priority', 'default edge buckets'],
  },
  gate: {
    label: 'Gate',
    oneLine: 'Checks required conditions and pauses or fails when they are not met.',
    useWhen: 'Use this before risky work or before declaring something complete.',
    setup: ['Add the required checks.', 'Choose whether blocked work pauses or fails.', 'Connect the happy path.'],
    advanced: ['Composite conditions', 'blocked behavior'],
  },
  human_checkpoint: {
    label: 'Human Checkpoint',
    oneLine: 'Pauses the Workflow and asks the user to choose a decision.',
    useWhen: 'Use this for judgment, auth, manual verification, or actions Xero should not decide alone.',
    setup: ['Write the decision prompt.', 'Add decision buttons.', 'Connect each decision to the next path.'],
    advanced: ['Resume payload schema', 'state updates after resume'],
  },
  merge: {
    label: 'Merge',
    oneLine: 'Waits for multiple incoming branches before continuing.',
    useWhen: 'Use this after parallel branches once the Workflow supports or uses fan-out work.',
    setup: ['Connect branches into the merge.', 'Choose the wait policy.', 'Connect the merged output.'],
    advanced: ['Quorum count', 'fail-fast behavior'],
  },
  terminal: {
    label: 'Done',
    oneLine: 'Ends the Workflow with a final status.',
    useWhen: 'Use this for success, failure, cancelled, or needs-human endings.',
    setup: ['Pick the final status.', 'Connect every route that should stop here.'],
    advanced: ['Terminal status'],
  },
  state_read: {
    label: 'Read State',
    oneLine: 'Loads durable project state into the run.',
    useWhen: 'Use this when later nodes need a known record, such as the current milestone.',
    setup: ['Choose what to read.', 'Add simple filters.', 'Name the handoff.'],
    advanced: ['JSON path filters', 'sort path', 'limit'],
  },
  state_query: {
    label: 'Find State',
    oneLine: 'Finds durable project records and saves the collection as a handoff.',
    useWhen: 'Use this for lists like incomplete phases, open requirements, or release candidates.',
    setup: ['Choose the record type.', 'Filter the list.', 'Name the handoff.'],
    advanced: ['JSON path filters', 'include archived', 'limit'],
  },
  state_write: {
    label: 'Write State',
    oneLine: 'Creates or updates durable project state.',
    useWhen: 'Use this when a run should remember a milestone, phase plan, verification result, or archive.',
    setup: ['Choose the record type.', 'Choose the action.', 'Fill the payload.'],
    advanced: ['Payload JSON', 'target id', 'idempotency key'],
  },
  state_patch: {
    label: 'Patch State',
    oneLine: 'Applies a controlled update to an existing durable state record.',
    useWhen: 'Use this for small updates when replacing the whole record would be too broad.',
    setup: ['Choose the record type.', 'Choose the target.', 'Fill the patch payload.'],
    advanced: ['Payload JSON', 'target id', 'idempotency key'],
  },
  state_checkpoint: {
    label: 'State Checkpoint',
    oneLine: 'Checks durable state and pauses or fails when state is not ready.',
    useWhen: 'Use this when the Workflow must stop until blockers or state gaps are resolved.',
    setup: ['Add state checks.', 'Choose whether blocked work pauses or fails.'],
    advanced: ['Composite conditions', 'blocked behavior'],
  },
  collection_loop: {
    label: 'Process Items',
    oneLine: 'Selects one item from a state collection and loops until the collection is done.',
    useWhen: 'Use this for queues, phase lists, bug lists, and release candidates.',
    setup: ['Choose the collection.', 'Choose ordering.', 'Set item limits.'],
    advanced: ['Only/from/to input paths', 'max runtime', 'requery behavior'],
  },
  subgraph: {
    label: 'Subflow',
    oneLine: 'Runs a reusable group of Workflow nodes.',
    useWhen: 'Use this to hide complexity and reuse a repeated pattern.',
    setup: ['Choose the subflow.', 'Map required inputs.', 'Name the subflow result.'],
    advanced: ['Input bindings', 'output contract'],
  },
  command: {
    label: 'Command',
    oneLine: 'Runs an allowlisted command and stores its output as evidence.',
    useWhen: 'Use this for checks such as tests, lint, git status, or project scripts.',
    setup: ['Enter the command.', 'Confirm the allowlist.', 'Choose how to parse output.'],
    advanced: ['Arguments', 'timeout', 'working directory', 'render path'],
  },
}

export const WORKFLOW_EDGE_HELP: Record<WorkflowEdgeTypeDto, WorkflowEdgeHelp> = {
  success: {
    label: 'After Success',
    oneLine: 'Runs this path when the previous node succeeds.',
    useWhen: 'Use this for the normal happy path.',
  },
  failure: {
    label: 'On Failure',
    oneLine: 'Runs this path when the previous node fails.',
    useWhen: 'Use this for human escalation, debug, or abort paths.',
  },
  conditional: {
    label: 'If Condition',
    oneLine: 'Runs this path only when a handoff or state condition matches.',
    useWhen: 'Use this for pass/fail routing, queue empty checks, or review decisions.',
  },
  loop: {
    label: 'Loop With Limit',
    oneLine: 'Runs a path again until the condition stops matching or attempts are exhausted.',
    useWhen: 'Use this for bounded retry, fix, review, or item-processing loops.',
  },
  recovery: {
    label: 'Recovery',
    oneLine: 'Runs this path to repair, debug, or close gaps.',
    useWhen: 'Use this when failure should trigger corrective work instead of ending the run.',
  },
  manual_override: {
    label: 'Manual Decision',
    oneLine: 'Runs after a human checkpoint returns a matching decision.',
    useWhen: 'Use this for choices like continue, stop, approve, or send back.',
  },
}

export const WORKFLOW_HANDOFF_PRESETS: WorkflowHandoffPreset[] = [
  {
    artifactType: 'text_output',
    label: 'Plain Text',
    description: 'Flexible text for summaries and final answers.',
    extraction: 'generic_text',
  },
  {
    artifactType: 'task_brief',
    label: 'Task Brief',
    description: 'A structured brief that can feed planning or execution.',
    extraction: 'json_object',
  },
  {
    artifactType: 'delivery_plan',
    label: 'Delivery Plan',
    description: 'A structured plan with steps, risks, and expected output.',
    extraction: 'json_object',
  },
  {
    artifactType: 'implementation_summary',
    label: 'Implementation Summary',
    description: 'What changed and what evidence was produced.',
    extraction: 'json_object',
  },
  {
    artifactType: 'verification_result',
    label: 'Verification Result',
    description: 'Pass, gaps found, human needed, or failed verification status.',
    extraction: 'json_object',
  },
  {
    artifactType: 'review_findings',
    label: 'Review Findings',
    description: 'Structured review counts and findings for routing fixes.',
    extraction: 'json_object',
  },
  {
    artifactType: 'gap_list',
    label: 'Gap List',
    description: 'A structured list of missing work to close before retrying.',
    extraction: 'json_object',
  },
  {
    artifactType: 'debug_report',
    label: 'Debug Report',
    description: 'Failure analysis plus a recommended route.',
    extraction: 'json_object',
  },
  {
    artifactType: 'command_result',
    label: 'Command Result',
    description: 'Captured command output and exit status.',
    extraction: 'json_object',
  },
]

export function workflowNodePlainSummary(
  node: WorkflowNodeDto,
  agentLabel: string | null = null,
): string {
  switch (node.type) {
    case 'agent':
      return `Runs ${agentLabel ?? 'the selected agent'} and saves ${handoffLabel(node.outputContract.artifactType)}.`
    case 'router':
      return 'Chooses the next path from the conditions on its outgoing connections.'
    case 'gate':
    case 'state_checkpoint':
      return `${node.requiredChecks.length} required check${node.requiredChecks.length === 1 ? '' : 's'}; blocked work will ${node.onBlocked}.`
    case 'human_checkpoint':
      return `Pauses for ${humanizeToken(node.checkpointType)} with ${node.decisionOptions.length || 1} decision option${node.decisionOptions.length === 1 ? '' : 's'}.`
    case 'merge':
      return `Waits for ${humanizeToken(node.waitPolicy)} incoming branch${node.waitPolicy === 'any' ? '' : 'es'} before continuing.`
    case 'terminal':
      return `Ends the Workflow as ${humanizeToken(node.terminalStatus)}.`
    case 'state_read':
    case 'state_query':
      return workflowStateQueryPlainSummary(node.query)
    case 'state_write':
    case 'state_patch':
      return workflowStateWritePlainSummary(node.operation)
    case 'collection_loop':
      return `Processes ${humanizeToken(node.collection.entityType)} one at a time, up to ${formatNumber(node.maxItemCount)} items.`
    case 'subgraph':
      return `Runs the ${node.subgraphId} subflow and saves ${handoffLabel(node.outputContract.artifactType)}.`
    case 'command':
      return `Runs ${formatCommand([node.command, ...node.args])} and saves ${handoffLabel(node.outputContract.artifactType)}.`
  }
}

export function workflowEdgePlainSummary(
  edge: WorkflowEdgeDto,
  definition: Pick<WorkflowDefinitionDto, 'nodes'>,
): string {
  const target = definition.nodes.find((node) => node.id === edge.toNodeId)
  const targetLabel = target?.title ?? edge.toNodeId
  return `${WORKFLOW_EDGE_HELP[edge.type].label}: ${workflowConditionPlainSummary(edge.condition)} -> ${targetLabel}.`
}

export function workflowStateQueryPlainSummary(query: WorkflowStateQueryDto): string {
  const filters = query.filters.length > 0
    ? ` where ${query.filters.map(workflowStateFilterPlainSummary).join(' and ')}`
    : ''
  const order = query.orderBy ? `, sorted by ${jsonPathLabel(query.orderBy)}` : ''
  const limit = query.limit ? `, limit ${formatNumber(query.limit)}` : ''
  return `Find ${humanizeToken(query.entityType)} records${filters}${order}${limit}.`
}

export function workflowStateWritePlainSummary(operation: WorkflowStateWriteOperationDto): string {
  const target = operation.targetId ? ` for ${operation.targetId}` : ''
  return `${humanizeToken(operation.action)} ${humanizeToken(operation.entityType)} state${target}.`
}

export function workflowStateFilterPlainSummary(filter: WorkflowStateQueryFilterDto): string {
  const field = jsonPathLabel(filter.path)
  switch (filter.operator) {
    case 'eq':
      return `${field} is ${formatJsonValue(filter.value)}`
    case 'neq':
      return `${field} is not ${formatJsonValue(filter.value)}`
    case 'in':
      return `${field} is one of ${formatJsonValues(filter.values ?? [])}`
    case 'not_in':
      return `${field} is not one of ${formatJsonValues(filter.values ?? [])}`
    case 'exists':
      return `${field} exists`
    case 'missing':
      return `${field} is missing`
  }
}

export function workflowConditionPlainSummary(condition: WorkflowConditionDto): string {
  switch (condition.kind) {
    case 'always':
      return 'always'
    case 'all':
      return condition.conditions.map(workflowConditionPlainSummary).join(' and ')
    case 'any':
      return condition.conditions.map(workflowConditionPlainSummary).join(' or ')
    case 'not':
      return `not (${workflowConditionPlainSummary(condition.condition)})`
    case 'node_status':
      return `${humanizeToken(condition.nodeId)} is ${humanizeToken(condition.status)}`
    case 'artifact_exists':
      return `${handoffRefLabel(condition.artifactRef)} exists`
    case 'artifact_field_equals':
      return `${handoffRefLabel(condition.artifactRef)} ${jsonPathLabel(condition.path)} is ${formatJsonValue(condition.value)}`
    case 'artifact_field_in':
      return `${handoffRefLabel(condition.artifactRef)} ${jsonPathLabel(condition.path)} is one of ${formatJsonValues(condition.values)}`
    case 'artifact_field_number_compare':
      return `${handoffRefLabel(condition.artifactRef)} ${jsonPathLabel(condition.path)} ${numberOperatorLabel(condition.operator)} ${formatNumber(condition.value)}`
    case 'failure_class_is':
      return `failure class is ${condition.failureClass}`
    case 'loop_attempt_lt':
      return `${humanizeToken(condition.loopKey)} attempts < ${formatNumber(condition.value)}`
    case 'loop_attempt_gte':
      return `${humanizeToken(condition.loopKey)} attempts >= ${formatNumber(condition.value)}`
    case 'human_decision_is':
      return `${humanizeToken(condition.checkpointNodeId)} decision is ${humanizeToken(condition.decision)}`
    case 'state_field_equals':
      return `${handoffRefLabel(condition.stateRef)} ${jsonPathLabel(condition.path)} is ${formatJsonValue(condition.value)}`
    case 'state_collection_count_compare':
      return `${handoffRefLabel(condition.stateRef)} count ${numberOperatorLabel(condition.operator)} ${formatNumber(condition.value)}`
  }
}

export function workflowInputBindingPlainSummary(binding: WorkflowInputBindingDto): string {
  const label = binding.promptLabel ?? humanizeToken(binding.name)
  const required = binding.required ? 'required' : 'optional'
  if (binding.source === 'run_input') {
    return `${label} from start input (${required})`
  }
  if (binding.source === 'artifact') {
    return `${label} from ${handoffRefLabel(binding.artifactRef)} (${required})`
  }
  return `${label} from ${handoffRefLabel(binding.stateRef)} (${required})`
}

export function handoffLabel(artifactType: string): string {
  return WORKFLOW_HANDOFF_PRESETS.find((preset) => preset.artifactType === artifactType)?.label
    ?? humanizeToken(artifactType)
}

export function handoffRefLabel(ref: string): string {
  const [nodeId, artifactType] = ref.split('.', 2)
  if (!artifactType) return humanizeToken(ref)
  return `${humanizeToken(nodeId)} ${handoffLabel(artifactType)}`
}

export function jsonPathLabel(path: string): string {
  return path
    .replace(/^\$\./, '')
    .replace(/^\$/, 'value')
    .replace(/\[\d+\]/g, '')
    .split(/[.[\]]+/)
    .filter(Boolean)
    .map(humanizeToken)
    .join(' ')
}

export function humanizeToken(value: string): string {
  return value
    .replace(/[_-]+/g, ' ')
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .trim()
    .replace(/\b\w/g, (letter) => letter.toUpperCase())
}

function numberOperatorLabel(operator: string): string {
  switch (operator) {
    case 'eq':
      return '='
    case 'neq':
      return '!='
    case 'gt':
      return '>'
    case 'gte':
      return '>='
    case 'lt':
      return '<'
    case 'lte':
      return '<='
    default:
      return operator
  }
}

function formatCommand(parts: readonly string[]): string {
  return parts.filter(Boolean).join(' ')
}

function formatJsonValues(values: readonly unknown[]): string {
  if (values.length === 0) return 'nothing'
  return values.map(formatJsonValue).join(', ')
}

function formatJsonValue(value: unknown): string {
  if (typeof value === 'string') return humanizeToken(value)
  if (typeof value === 'number') return formatNumber(value)
  if (typeof value === 'boolean') return value ? 'true' : 'false'
  if (value === null || value === undefined) return 'blank'
  return JSON.stringify(value)
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 }).format(value)
}
