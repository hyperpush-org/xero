import type { RuntimeAgentIdDto } from '@/src/lib/xero-model'

const ROUTING_MARKER_PATTERN = String.raw`<xero-routing-suggestion\b([\s\S]*?)(?:\/>|>\s*<\/xero-routing-suggestion\s*>|>)`
const ROUTING_MARKER_REGEX = new RegExp(ROUTING_MARKER_PATTERN, 'i')
const ROUTING_MARKER_GLOBAL_REGEX = new RegExp(ROUTING_MARKER_PATTERN, 'gi')
const ELIGIBLE_BUILT_IN_TARGETS = new Set<RuntimeAgentIdDto>([
  'ask',
  'plan',
  'engineer',
  'debug',
  'generalist',
])

export interface ParsedRoutingMarker {
  targetKind: 'built_in' | 'custom'
  targetAgentId: RuntimeAgentIdDto
  targetAgentDefinitionId: string | null
  targetAgentDefinitionVersion: number | null
  targetLabel: string | null
  reason: string
  summary: string
  rawMarker: string
}

function parseMarkerAttributes(attrs: string): Record<string, string> {
  const parsed: Record<string, string> = {}
  const attrRegex = /([a-zA-Z][\w:-]*)\s*=\s*(?:"([^"]*)"|'([^']*)'|“([^”]*)”|‘([^’]*)’|([^\s/>]+))/g
  let match: RegExpExecArray | null = null
  while ((match = attrRegex.exec(attrs)) !== null) {
    parsed[match[1]] = (match[2] ?? match[3] ?? match[4] ?? match[5] ?? match[6] ?? '').trim()
  }
  return parsed
}

function parseEligibleBuiltInTarget(value: string | undefined): RuntimeAgentIdDto | null {
  const trimmed = value?.toLowerCase().trim() ?? ''
  if (!ELIGIBLE_BUILT_IN_TARGETS.has(trimmed as RuntimeAgentIdDto)) {
    return null
  }
  return trimmed as RuntimeAgentIdDto
}

function parsePositiveInteger(value: string | undefined): number | null {
  const trimmed = value?.trim() ?? ''
  if (trimmed.length === 0) return null
  const parsed = Number.parseInt(trimmed, 10)
  return Number.isInteger(parsed) && parsed > 0 ? parsed : null
}

export function parseRoutingMarker(text: string): ParsedRoutingMarker | null {
  const match = text.match(ROUTING_MARKER_REGEX)
  if (!match) return null
  const attrs = parseMarkerAttributes(match[1])
  const targetKind = (attrs.targetKind ?? attrs.kind ?? 'built_in').toLowerCase().trim()
  const reason = attrs.reason?.trim() ?? ''
  const summary = attrs.summary?.trim() ?? ''

  if (targetKind === 'custom') {
    const definitionId = attrs.definitionId?.trim() ?? ''
    if (definitionId.length === 0) {
      return null
    }
    return {
      targetKind: 'custom',
      targetAgentId:
        parseEligibleBuiltInTarget(attrs.runtimeAgentId ?? attrs.target) ?? 'generalist',
      targetAgentDefinitionId: definitionId,
      targetAgentDefinitionVersion: parsePositiveInteger(
        attrs.definitionVersion ?? attrs.version,
      ),
      targetLabel: attrs.targetLabel?.trim() || attrs.label?.trim() || null,
      reason,
      summary,
      rawMarker: match[0],
    }
  }

  if (targetKind !== 'built_in') {
    return null
  }
  const targetAgentId = parseEligibleBuiltInTarget(attrs.target ?? attrs.runtimeAgentId)
  if (!targetAgentId) {
    return null
  }
  return {
    targetKind: 'built_in',
    targetAgentId,
    targetAgentDefinitionId: null,
    targetAgentDefinitionVersion: null,
    targetLabel: null,
    reason,
    summary,
    rawMarker: match[0],
  }
}

export function stripRoutingMarkers(text: string): string {
  return text.replace(ROUTING_MARKER_GLOBAL_REGEX, '').replace(/\n{3,}/g, '\n\n').trim()
}
