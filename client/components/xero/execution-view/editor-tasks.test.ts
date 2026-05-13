import { describe, expect, it } from 'vitest'
import {
  appendEditorTaskOutput,
  buildEditorTaskDefinitions,
  parseEditorTaskProblems,
} from './editor-tasks'

describe('editor task problem matchers', () => {
  it('parses TypeScript diagnostics', () => {
    const diagnostics = parseEditorTaskProblems(
      "src/app.tsx(3,7): error TS2322: Type 'string' is not assignable to type 'number'.\n",
    )

    expect(diagnostics).toMatchObject([
      {
        path: '/src/app.tsx',
        line: 3,
        column: 7,
        severity: 'error',
        code: 'TS2322',
        source: 'typescript',
      },
    ])
  })

  it('parses ESLint stylish output', () => {
    const diagnostics = parseEditorTaskProblems(
      [
        '/Users/sn0w/project/src/index.ts',
        '  4:2  warning  foo is defined but never used  no-unused-vars',
      ].join('\n'),
      { projectRoot: '/Users/sn0w/project' },
    )

    expect(diagnostics).toMatchObject([
      {
        path: '/src/index.ts',
        line: 4,
        column: 2,
        severity: 'warning',
        code: 'no-unused-vars',
        source: 'eslint',
      },
    ])
  })

  it('parses Rust compiler diagnostics', () => {
    const diagnostics = parseEditorTaskProblems(
      [
        'error[E0308]: mismatched types',
        '  --> src/main.rs:12:5',
        '   |',
      ].join('\n'),
    )

    expect(diagnostics).toMatchObject([
      {
        path: '/src/main.rs',
        line: 12,
        column: 5,
        severity: 'error',
        code: 'E0308',
        source: 'rustc',
      },
    ])
  })

  it('parses Go and Python generic file-line-column output', () => {
    const diagnostics = parseEditorTaskProblems(
      [
        'pkg/server.go:10:2: undefined: handler',
        'app/models.py:8: error: Name "User" is not defined  [name-defined]',
      ].join('\n'),
    )

    expect(diagnostics).toMatchObject([
      {
        path: '/pkg/server.go',
        line: 10,
        column: 2,
        severity: 'error',
        source: 'go',
      },
      {
        path: '/app/models.py',
        line: 8,
        column: 1,
        severity: 'error',
        source: 'python',
      },
    ])
  })

  it('builds first-class editor tasks plus configured start targets', () => {
    const tasks = buildEditorTaskDefinitions([
      { id: 'web', name: 'web', command: 'pnpm dev' },
    ])

    expect(tasks.map((task) => task.id)).toEqual([
      'typecheck',
      'lint',
      'test',
      'build',
      'start:web',
    ])
    expect(tasks.find((task) => task.id === 'start:web')).toMatchObject({
      label: 'Start: web',
      terminalLabel: 'start: web',
      command: 'pnpm dev',
    })
  })

  it('keeps bounded terminal output buffers', () => {
    const current = 'a'.repeat(1024 * 1024)
    const result = appendEditorTaskOutput(current, 'tail')

    expect(result.truncated).toBe(true)
    expect(result.output.endsWith('tail')).toBe(true)
    expect(result.output.length).toBe(1024 * 1024)
  })
})
