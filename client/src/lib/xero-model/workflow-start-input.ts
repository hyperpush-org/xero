import type {
  WorkflowDefinitionDto,
  WorkflowInputBindingDto,
  WorkflowNodeDto,
} from './workflow-definition'

export interface WorkflowStartInputField {
  /** Stable form key and top-level initial-input property. */
  key: string
  /** Binding name used when no explicit simple path is configured. */
  name: string
  /** Effective run-input path. Omitted paths resolve to `$.<name>`. */
  path: string
  label: string
  required: boolean
}

export interface WorkflowStartInputPlan {
  fields: WorkflowStartInputField[]
  requiredFields: WorkflowStartInputField[]
  hasRequiredInput: boolean
}

type WorkflowInputBindingNode = Extract<
  WorkflowNodeDto,
  { type: 'agent' | 'state_write' | 'state_patch' | 'subgraph' }
>

/**
 * Collect every run input referenced by the top-level graph or a declared
 * subgraph. Inputs that resolve to the same top-level field are shown once.
 */
export function getWorkflowStartInputPlan(
  definition: Pick<WorkflowDefinitionDto, 'nodes' | 'subgraphs'>,
): WorkflowStartInputPlan {
  const fields = new Map<string, WorkflowStartInputField & { hasExplicitLabel: boolean }>()

  const visitBindings = (bindings: readonly WorkflowInputBindingDto[]) => {
    for (const binding of bindings) {
      if (binding.source !== 'run_input') continue

      const path = workflowStartInputPath(binding.name, binding.path ?? null)
      const key = workflowStartInputKey(binding.name, path)
      const previous = fields.get(key)
      const hasExplicitLabel = Boolean(binding.promptLabel)

      fields.set(key, {
        key,
        name: previous?.name ?? binding.name,
        path: previous?.path ?? path,
        label:
          previous?.hasExplicitLabel === true
            ? previous.label
            : binding.promptLabel ?? previous?.label ?? humanizeInputName(binding.name),
        required: Boolean(previous?.required || binding.required),
        hasExplicitLabel: Boolean(previous?.hasExplicitLabel || hasExplicitLabel),
      })
    }
  }

  for (const node of definition.nodes) {
    if (hasWorkflowInputBindings(node)) visitBindings(node.inputBindings)
  }
  for (const subgraph of definition.subgraphs) {
    visitBindings(subgraph.inputBindings)
    for (const node of subgraph.nodes) {
      if (hasWorkflowInputBindings(node)) visitBindings(node.inputBindings)
    }
  }

  const orderedFields = [...fields.values()]
    .map(({ hasExplicitLabel: _hasExplicitLabel, ...field }) => field)
    .sort(
      (left, right) =>
        Number(right.required) - Number(left.required) || left.label.localeCompare(right.label),
    )
  const requiredFields = orderedFields.filter((field) => field.required)

  return {
    fields: orderedFields,
    requiredFields,
    hasRequiredInput: requiredFields.length > 0,
  }
}

/** Build the JSON value accepted by Workflow start from collected form values. */
export function buildWorkflowInitialInput(
  fields: readonly WorkflowStartInputField[],
  values: Readonly<Record<string, string>>,
): unknown {
  let input: unknown = {}
  for (const field of fields) {
    const value = (values[field.key] ?? '').trim()
    if (!value) continue
    input = setWorkflowInitialInputValue(input, field.path, value)
  }
  return input
}

/** Resolve the path used by start validation for an omitted binding path. */
export function workflowStartInputPath(name: string, path: string | null): string {
  return path ?? `$.${name}`
}

/** Use concise keys for top-level fields and the full path for structured inputs. */
export function workflowStartInputKey(_name: string, path: string): string {
  const field = path.match(/^\$\.([A-Za-z0-9_-]+)$/)?.[1]
  return field && !isUnsafeWorkflowInputField(field) ? field : path
}

type WorkflowInputPathSegment = string | number
const MAX_WORKFLOW_INPUT_PATH_LENGTH = 2_048
const MAX_WORKFLOW_INPUT_PATH_SEGMENTS = 32
const MAX_WORKFLOW_INPUT_ARRAY_INDEX = 1_024

function setWorkflowInitialInputValue(root: unknown, path: string, value: string): unknown {
  if (path === '$') {
    if (!isEmptyInitialInputRoot(root)) {
      throw new Error('A Workflow root input cannot be combined with other start inputs.')
    }
    return value
  }

  const segments = parseWorkflowInputPath(path)
  if (!segments) {
    throw new Error(`Workflow input path \`${path}\` is invalid.`)
  }
  if (!isContainer(root)) {
    throw new Error('A Workflow root input cannot be combined with nested start inputs.')
  }

  let cursor: Record<string, unknown> | unknown[] = root
  for (let index = 0; index < segments.length; index += 1) {
    const segment = segments[index]
    const isLast = index === segments.length - 1
    const nextSegment = segments[index + 1]

    if (typeof segment === 'number') {
      if (!Array.isArray(cursor)) {
        throw new Error(`Workflow input path \`${path}\` conflicts with another start input.`)
      }
      if (isLast) {
        if (isContainer(cursor[segment])) {
          throw new Error(`Workflow input path \`${path}\` conflicts with another start input.`)
        }
        cursor[segment] = value
        continue
      }
      const next = cursor[segment]
      const expectedArray = typeof nextSegment === 'number'
      if (next === undefined || next === null) {
        cursor[segment] = expectedArray ? [] : {}
      } else if (!isContainer(next) || Array.isArray(next) !== expectedArray) {
        throw new Error(`Workflow input path \`${path}\` conflicts with another start input.`)
      }
      cursor = cursor[segment] as Record<string, unknown> | unknown[]
      continue
    }

    if (Array.isArray(cursor)) {
      throw new Error(`Workflow input path \`${path}\` conflicts with another start input.`)
    }
    if (isLast) {
      if (isContainer(cursor[segment])) {
        throw new Error(`Workflow input path \`${path}\` conflicts with another start input.`)
      }
      cursor[segment] = value
      continue
    }
    const next = cursor[segment]
    const expectedArray = typeof nextSegment === 'number'
    if (next === undefined || next === null) {
      cursor[segment] = expectedArray ? [] : {}
    } else if (!isContainer(next) || Array.isArray(next) !== expectedArray) {
      throw new Error(`Workflow input path \`${path}\` conflicts with another start input.`)
    }
    cursor = cursor[segment] as Record<string, unknown> | unknown[]
  }

  return root
}

function parseWorkflowInputPath(path: string): WorkflowInputPathSegment[] | null {
  if (path.length > MAX_WORKFLOW_INPUT_PATH_LENGTH) return null
  const remainder = path.startsWith('$.') ? path.slice(2) : null
  if (!remainder) return null

  const segments: WorkflowInputPathSegment[] = []
  for (const rawSegment of remainder.split('.')) {
    const match = rawSegment.match(/^([^\[\]]+)((?:\[\d+\])*)$/)
    if (!match) return null
    const field = match[1]
    if (isUnsafeWorkflowInputField(field)) {
      return null
    }
    segments.push(field)
    for (const indexMatch of match[2].matchAll(/\[(\d+)\]/g)) {
      const arrayIndex = Number.parseInt(indexMatch[1], 10)
      if (!Number.isSafeInteger(arrayIndex) || arrayIndex > MAX_WORKFLOW_INPUT_ARRAY_INDEX) {
        return null
      }
      segments.push(arrayIndex)
    }
    if (segments.length > MAX_WORKFLOW_INPUT_PATH_SEGMENTS) return null
  }
  return segments.length > 0 ? segments : null
}

function isUnsafeWorkflowInputField(field: string): boolean {
  return field === '__proto__' || field === 'prototype' || field === 'constructor'
}

function isContainer(value: unknown): value is Record<string, unknown> | unknown[] {
  return typeof value === 'object' && value !== null
}

function isEmptyInitialInputRoot(value: unknown): boolean {
  if (Array.isArray(value)) return value.length === 0
  return isContainer(value) && Object.keys(value).length === 0
}

function hasWorkflowInputBindings(node: WorkflowNodeDto): node is WorkflowInputBindingNode {
  return (
    node.type === 'agent' ||
    node.type === 'state_write' ||
    node.type === 'state_patch' ||
    node.type === 'subgraph'
  )
}

function humanizeInputName(value: string): string {
  return value
    .replace(/[_-]+/g, ' ')
    .replace(/\b\w/g, (letter) => letter.toUpperCase())
}
