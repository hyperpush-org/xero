#!/usr/bin/env node

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')

const read = (path) => readFileSync(resolve(repoRoot, path), 'utf8')

const planPath = 'AGENT_SYSTEM_IMPROVEMENT_PLAN.md'
const releasePath = 'docs/agent-system-release-checklist.md'
const dogfoodPath = 'docs/agent-system-dogfood-notes.md'

const plan = read(planPath)
const release = read(releasePath)
const dogfood = read(dogfoodPath)

const errors = []

function fail(message) {
  errors.push(message)
}

function sectionEnd(text, start) {
  const next = text.indexOf('\n## ', start)
  return next === -1 ? text.length : next
}

function parseSlices(text) {
  const heading = /^- \[[ x]\] (S\d+) - ([^\n]+)$/gm
  return [...text.matchAll(heading)].map((match) => ({
    id: match[1],
    title: match[2],
    checked: match[0].startsWith('- [x]'),
    start: match.index,
    endHeading: match.index + match[0].length,
  }))
}

function sliceSort(ids) {
  return [...ids].sort((a, b) => Number(a.slice(1)) - Number(b.slice(1)))
}

function sliceSetLabel(ids) {
  return sliceSort(ids).join(', ')
}

function expandSliceReferences(text) {
  const ids = new Set()
  const reference = /S(\d+)(?:-S?(\d+))?/g
  for (const match of text.matchAll(reference)) {
    const start = Number.parseInt(match[1], 10)
    const end = match[2] ? Number.parseInt(match[2], 10) : start
    for (let value = start; value <= end; value += 1) {
      ids.add(`S${String(value).padStart(2, '0')}`)
    }
  }
  return ids
}

function assertSameSliceSet(label, actual, expected) {
  const missing = sliceSort([...expected].filter((id) => !actual.has(id)))
  const extra = sliceSort([...actual].filter((id) => !expected.has(id)))
  if (missing.length > 0 || extra.length > 0) {
    fail(
      `${label} does not match the actual unchecked slice set. Missing: ${
        missing.join(', ') || 'none'
      }. Extra: ${extra.join(', ') || 'none'}.`,
    )
  }
}

function tableRows(section) {
  return section
    .split('\n')
    .filter((line) => line.startsWith('| '))
    .filter((line) => !/^\|[-: |]+\|$/.test(line))
    .map((line) =>
      line
        .slice(1, -1)
        .split('|')
        .map((cell) => cell.trim()),
    )
}

const slices = parseSlices(plan)
const uncheckedIds = new Set(slices.filter((slice) => !slice.checked).map((slice) => slice.id))
const s70 = slices.find((slice) => slice.id === 'S70')

slices.forEach((slice, index) => {
  const nextSlice = slices[index + 1]?.start ?? plan.length
  const nextMilestone = plan.indexOf('\n## ', slice.endHeading)
  const end = nextMilestone >= 0 && nextMilestone < nextSlice ? nextMilestone : nextSlice
  const body = plan.slice(slice.endHeading, end)

  if (slice.checked) {
    const required = {
      'completed behavior': /Completed behavior:/i,
      'runtime/storage contract': /Runtime\/storage contract:|Runtime contract:|Storage contract:/i,
      verification: /Verification/i,
      'rollout consequence': /Rollout consequence:/i,
    }

    Object.entries(required).forEach(([label, pattern]) => {
      if (!pattern.test(body)) {
        fail(`${slice.id} is checked but is missing ${label} evidence.`)
      }
    })
  } else {
    const hasReason =
      /UI-deferred|Remaining:|dogfood|backend-only|explicitly accepted|explicitly waived/i.test(
        body,
      )
    if (!hasReason) {
      fail(`${slice.id} is unchecked but has no explicit deferral or remaining-work reason.`)
    }

    if (!release.includes(slice.id)) {
      fail(`${slice.id} is unchecked but is not referenced in ${releasePath}.`)
    }
    if (!dogfood.includes(slice.id)) {
      fail(`${slice.id} is unchecked but is not referenced in ${dogfoodPath}.`)
    }
  }
})

const implementationBoundaryMatch = plan.match(
  /remaining unchecked slices are intentionally not complete[^:]*: ([^.]+)\./,
)
if (!implementationBoundaryMatch) {
  fail(`${planPath} is missing the Current Implementation Boundary unchecked-slice list.`)
} else {
  assertSameSliceSet(
    'Current Implementation Boundary unchecked-slice list',
    expandSliceReferences(implementationBoundaryMatch[1]),
    uncheckedIds,
  )
}

const completionAuditMatch = plan.match(/Current slice scan still shows unchecked ([^.]+)\./)
if (!completionAuditMatch) {
  fail(`${planPath} is missing the prompt-to-artifact unchecked-slice scan list.`)
} else {
  assertSameSliceSet(
    'Prompt-To-Artifact unchecked-slice scan list',
    expandSliceReferences(completionAuditMatch[1]),
    uncheckedIds,
  )
}

const releaseCoverageStart = release.indexOf('## Unchecked Slice Coverage')
if (releaseCoverageStart === -1) {
  fail(`${releasePath} is missing the Unchecked Slice Coverage section.`)
} else {
  const releaseCoverage = release.slice(
    releaseCoverageStart,
    sectionEnd(release, releaseCoverageStart + 1),
  )
  assertSameSliceSet(
    `${releasePath} Unchecked Slice Coverage`,
    expandSliceReferences(releaseCoverage),
    uncheckedIds,
  )
}

const definitionHeading = '## Definition Of "Good Enough"'
const definitionStart = plan.indexOf(definitionHeading)
if (definitionStart === -1) {
  fail(`${planPath} is missing ${definitionHeading}.`)
} else {
  const definition = plan.slice(definitionStart, sectionEnd(plan, definitionStart + 1))
  const auditStart = definition.indexOf('### Good Enough Coverage Audit')
  if (auditStart === -1) {
    fail(`${planPath} is missing the Good Enough Coverage Audit.`)
  } else {
    const beforeAudit = definition.slice(0, auditStart)
    const audit = definition.slice(auditStart)
    const criteria = [...beforeAudit.matchAll(/^- (.+)$/gm)].map((match) => match[1])

    if (criteria.length === 0) {
      fail('Definition Of "Good Enough" has no criteria bullets.')
    }

    criteria.forEach((criterion) => {
      if (!audit.includes(`| ${criterion} |`)) {
        fail(`Good Enough criterion is missing from the coverage audit: ${criterion}`)
      }
    })
  }
}

const acceptanceNeedles = [
  '## Backend-Only Acceptance Decision',
  'No backend-only acceptance decision is recorded',
  'decision owner',
  'unchecked slice ids',
]

;[
  [releasePath, release],
  [dogfoodPath, dogfood],
].forEach(([path, text]) => {
  acceptanceNeedles.forEach((needle) => {
    if (!text.includes(needle)) {
      fail(`${path} is missing backend-only acceptance marker: ${needle}`)
    }
  })
})

if (!plan.includes('Overall audit result: not complete.')) {
  fail(`${planPath} must explicitly state that the current audit result is not complete.`)
}

if (!dogfood.includes('No dogfood runs have been recorded yet.')) {
  fail(`${dogfoodPath} must preserve the current S70 no-runs blocker.`)
}

const requiredWorkflows = new Set([
  'Engineering',
  'Debugging',
  'Planning',
  'Repository reconnaissance',
  'Custom support triage',
  'Long-running handoff',
])
const workflowStart = dogfood.indexOf('## Required Workflows')
if (workflowStart === -1) {
  fail(`${dogfoodPath} is missing the Required Workflows section.`)
} else {
  const workflowSection = dogfood.slice(workflowStart, sectionEnd(dogfood, workflowStart + 1))
  const workflowRows = tableRows(workflowSection).filter((row) => row[0] !== 'Workflow')
  const seenWorkflows = new Set(workflowRows.map((row) => row[0]))
  requiredWorkflows.forEach((workflow) => {
    if (!seenWorkflows.has(workflow)) {
      fail(`${dogfoodPath} is missing required S70 workflow row: ${workflow}`)
    }
  })
  workflowRows.forEach((row) => {
    const workflow = row[0]
    const result = row[3]
    if (!requiredWorkflows.has(workflow)) {
      fail(`${dogfoodPath} has an unexpected S70 workflow row: ${workflow}`)
    }
    if (s70 && !s70.checked && result !== 'Not run') {
      fail(`${dogfoodPath} workflow ${workflow} must remain "Not run" while S70 is unchecked.`)
    }
  })
}

if (errors.length > 0) {
  console.error('[agent-system-plan] verification failed:')
  errors.forEach((error) => console.error(`- ${error}`))
  process.exit(1)
}

console.log('[agent-system-plan] plan, release checklist, and dogfood audit links are consistent.')
