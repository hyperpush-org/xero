export type AgentAttachmentKind = 'image' | 'document' | 'text'

export type AgentAttachmentClassification =
  | { kind: AgentAttachmentKind; mediaType: string }
  | { kind: null; reason: 'unsupported' | 'too_large' | 'empty' }

export const MAX_ATTACHMENT_BYTES = 20 * 1024 * 1024
export const MAX_TOTAL_ATTACHMENT_BYTES = 50 * 1024 * 1024

const IMAGE_MIME_TYPES = new Set<string>([
  'image/png',
  'image/jpeg',
  'image/jpg',
  'image/gif',
  'image/webp',
])

const DOCUMENT_MIME_TYPES = new Set<string>(['application/pdf'])

const TEXT_MIME_TYPES = new Set<string>([
  'application/json',
  'application/javascript',
  'application/x-typescript',
  'application/typescript',
  'application/xml',
  'application/x-yaml',
  'application/x-toml',
  'application/sql',
  'application/x-sh',
])

const EXTENSION_FALLBACKS: Record<string, string> = {
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  webp: 'image/webp',
  pdf: 'application/pdf',
  txt: 'text/plain',
  md: 'text/markdown',
  markdown: 'text/markdown',
  html: 'text/html',
  htm: 'text/html',
  css: 'text/css',
  csv: 'text/csv',
  json: 'application/json',
  js: 'application/javascript',
  mjs: 'application/javascript',
  cjs: 'application/javascript',
  ts: 'application/x-typescript',
  tsx: 'application/x-typescript',
  jsx: 'application/javascript',
  xml: 'application/xml',
  yml: 'application/x-yaml',
  yaml: 'application/x-yaml',
  toml: 'application/x-toml',
  sql: 'application/sql',
  sh: 'application/x-sh',
  bash: 'application/x-sh',
  zsh: 'application/x-sh',
  rs: 'text/plain',
  py: 'text/plain',
  go: 'text/plain',
  rb: 'text/plain',
  c: 'text/plain',
  h: 'text/plain',
  cpp: 'text/plain',
  hpp: 'text/plain',
  swift: 'text/plain',
  kt: 'text/plain',
  java: 'text/plain',
  log: 'text/plain',
  conf: 'text/plain',
  ini: 'text/plain',
  env: 'text/plain',
  dockerfile: 'text/plain',
}

export function classifyAttachment(file: {
  type: string
  name: string
  size: number
}): AgentAttachmentClassification {
  if (file.size === 0) {
    return { kind: null, reason: 'empty' }
  }
  if (file.size > MAX_ATTACHMENT_BYTES) {
    return { kind: null, reason: 'too_large' }
  }
  const mediaType = resolveMediaType(file.type, file.name)
  if (!mediaType) {
    return { kind: null, reason: 'unsupported' }
  }
  const lower = mediaType.toLowerCase()
  if (lower.startsWith('image/') && IMAGE_MIME_TYPES.has(lower)) {
    return { kind: 'image', mediaType: lower }
  }
  if (DOCUMENT_MIME_TYPES.has(lower)) {
    return { kind: 'document', mediaType: lower }
  }
  if (lower.startsWith('text/') || TEXT_MIME_TYPES.has(lower)) {
    return { kind: 'text', mediaType: lower }
  }
  return { kind: null, reason: 'unsupported' }
}

function resolveMediaType(reportedType: string, fileName: string): string | null {
  const trimmed = reportedType.trim().toLowerCase()
  if (trimmed && trimmed !== 'application/octet-stream') {
    return trimmed
  }
  return mediaTypeFromExtension(fileName)
}

function mediaTypeFromExtension(fileName: string): string | null {
  const lastDot = fileName.lastIndexOf('.')
  if (lastDot <= 0) {
    if (fileName.toLowerCase() === 'dockerfile') return EXTENSION_FALLBACKS.dockerfile
    return null
  }
  const ext = fileName.slice(lastDot + 1).toLowerCase()
  return EXTENSION_FALLBACKS[ext] ?? null
}

export function classificationRejectionMessage(
  file: { name: string; size: number },
  classification: Extract<AgentAttachmentClassification, { kind: null }>,
): string {
  switch (classification.reason) {
    case 'empty':
      return `Skipped "${file.name}" because it is empty.`
    case 'too_large':
      return `Skipped "${file.name}" because it is larger than ${formatBytes(MAX_ATTACHMENT_BYTES)} (got ${formatBytes(file.size)}).`
    case 'unsupported':
      return `Skipped "${file.name}" — that file type can't be sent to the agent yet.`
  }
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB']
  let value = bytes / 1024
  let unitIndex = 0
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unitIndex]}`
}
