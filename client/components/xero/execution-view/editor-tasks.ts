import type { ProjectDiagnosticDto, StartTargetDto } from '@/src/lib/xero-model'

export type EditorTaskKind = 'typecheck' | 'lint' | 'test' | 'build' | 'start'

export interface EditorTaskDefinition {
  id: string
  kind: EditorTaskKind
  label: string
  command: string
  terminalLabel: string
}

export interface EditorTerminalTaskExit {
  terminalId: string
  exitCode: number | null
}

export interface EditorTerminalTaskRequest {
  taskId: string
  kind: EditorTaskKind
  label: string
  command: string
  exitWhenDone?: boolean
  onData?: (data: string) => void
  onExit?: (event: EditorTerminalTaskExit) => void
}

export const MAX_EDITOR_TASK_OUTPUT_BYTES = 1024 * 1024

const PACKAGE_SCRIPT_HELPERS = String.raw`
has_package_script() {
  script_name="$1"
  [ -f package.json ] || return 1
  command -v node >/dev/null 2>&1 || return 1
  node -e 'const fs = require("fs"); const name = process.argv[1]; try { const pkg = JSON.parse(fs.readFileSync("package.json", "utf8")); const script = pkg.scripts && pkg.scripts[name]; process.exit(typeof script === "string" && script.trim() ? 0 : 1); } catch { process.exit(1); }' "$script_name"
}

run_package_script() {
  script_name="$1"
  if [ -f pnpm-lock.yaml ] && command -v pnpm >/dev/null 2>&1; then
    pnpm run "$script_name"
  elif [ -f yarn.lock ] && command -v yarn >/dev/null 2>&1; then
    yarn "$script_name"
  elif [ -f bun.lockb ] && command -v bun >/dev/null 2>&1; then
    bun run "$script_name"
  elif command -v npm >/dev/null 2>&1; then
    npm run "$script_name"
  else
    echo "No package manager was found for package.json script '$script_name'."
    return 127
  fi
}
`.trim()

export function appendEditorTaskOutput(
  current: string,
  chunk: string,
): { output: string; truncated: boolean } {
  const next = current + chunk
  if (next.length <= MAX_EDITOR_TASK_OUTPUT_BYTES) {
    return { output: next, truncated: false }
  }
  return {
    output: next.slice(next.length - MAX_EDITOR_TASK_OUTPUT_BYTES),
    truncated: true,
  }
}

export function buildEditorTaskDefinitions(
  startTargets: readonly StartTargetDto[] = [],
): EditorTaskDefinition[] {
  return [
    {
      id: 'typecheck',
      kind: 'typecheck',
      label: 'Typecheck',
      terminalLabel: 'task: typecheck',
      command: packageScriptTaskCommand(
        'typecheck',
        String.raw`
if [ -f tsconfig.json ]; then
  if [ -x node_modules/.bin/tsc ]; then
    ./node_modules/.bin/tsc --noEmit --pretty false
  elif command -v tsc >/dev/null 2>&1; then
    tsc --noEmit --pretty false
  else
    echo "No TypeScript compiler was found for typecheck."
    exit 127
  fi
else
  echo "No typecheck task or tsconfig.json was found for this project."
  exit 127
fi
`.trim(),
      ),
    },
    {
      id: 'lint',
      kind: 'lint',
      label: 'Lint',
      terminalLabel: 'task: lint',
      command: packageScriptTaskCommand(
        'lint',
        String.raw`
if [ -x node_modules/.bin/eslint ]; then
  ./node_modules/.bin/eslint .
elif [ -f pnpm-lock.yaml ] && command -v pnpm >/dev/null 2>&1; then
  pnpm exec eslint .
elif [ -f yarn.lock ] && command -v yarn >/dev/null 2>&1; then
  yarn eslint .
elif [ -f bun.lockb ] && command -v bun >/dev/null 2>&1; then
  bun x eslint .
elif command -v npx >/dev/null 2>&1; then
  npx --no-install eslint .
else
  echo "No lint task or ESLint executable was found for this project."
  exit 127
fi
`.trim(),
      ),
    },
    {
      id: 'test',
      kind: 'test',
      label: 'Test',
      terminalLabel: 'task: test',
      command: packageScriptTaskCommand(
        'test',
        String.raw`
if [ -f Cargo.toml ]; then
  cargo test
elif [ -f go.mod ]; then
  go test ./...
elif [ -f pyproject.toml ] || [ -f pytest.ini ] || [ -f setup.cfg ]; then
  python3 -m pytest
else
  echo "No test task was found for this project."
  exit 127
fi
`.trim(),
      ),
    },
    {
      id: 'build',
      kind: 'build',
      label: 'Build',
      terminalLabel: 'task: build',
      command: packageScriptTaskCommand(
        'build',
        String.raw`
if [ -f Cargo.toml ]; then
  cargo build
elif [ -f go.mod ]; then
  go build ./...
else
  echo "No build task was found for this project."
  exit 127
fi
`.trim(),
      ),
    },
    ...startTargets.map((target) => ({
      id: `start:${target.id}`,
      kind: 'start' as const,
      label: `Start: ${target.name}`,
      terminalLabel: `start: ${target.name}`,
      command: target.command,
    })),
  ]
}

function packageScriptTaskCommand(scriptName: string, fallback: string): string {
  return `${PACKAGE_SCRIPT_HELPERS}

if has_package_script '${scriptName}'; then
  run_package_script '${scriptName}'
else
${indent(fallback, '  ')}
fi`
}

function indent(value: string, prefix: string): string {
  return value
    .split('\n')
    .map((line) => `${prefix}${line}`)
    .join('\n')
}

interface ParseEditorTaskProblemsOptions {
  projectRoot?: string | null
}

type DiagnosticSeverity = ProjectDiagnosticDto['severity']

const SOURCE_PATH_EXTENSIONS = new Set([
  'c',
  'cc',
  'cpp',
  'cxx',
  'go',
  'h',
  'hh',
  'hpp',
  'hxx',
  'js',
  'jsx',
  'json',
  'm',
  'mm',
  'mjs',
  'mts',
  'py',
  'pyi',
  'rs',
  'ts',
  'tsx',
  'vue',
])

export function parseEditorTaskProblems(
  output: string,
  options: ParseEditorTaskProblemsOptions = {},
): ProjectDiagnosticDto[] {
  const clean = stripTerminalControlSequences(output)
  const lines = clean.split('\n')
  const diagnostics: ProjectDiagnosticDto[] = []
  const seen = new Set<string>()
  let eslintFile: string | null = null
  let rustDiagnostic: { severity: DiagnosticSeverity; code: string | null; message: string } | null =
    null
  let pythonFrame: { path: string; line: number } | null = null

  const addDiagnostic = (diagnostic: ProjectDiagnosticDto) => {
    const key = [
      diagnostic.source,
      diagnostic.path ?? '',
      diagnostic.line ?? '',
      diagnostic.column ?? '',
      diagnostic.severity,
      diagnostic.code ?? '',
      diagnostic.message,
    ].join('\u0000')
    if (seen.has(key)) return
    seen.add(key)
    diagnostics.push(diagnostic)
  }

  for (const rawLine of lines) {
    const line = rawLine.trimEnd()
    const trimmed = line.trim()
    if (!trimmed) continue

    const tsMatch = trimmed.match(
      /^(.+?)\((\d+),(\d+)\):\s+(error|warning)\s+TS(\d+):\s+(.+)$/,
    )
    if (tsMatch) {
      const path = normalizeProblemPath(tsMatch[1] ?? '', options.projectRoot)
      if (path) {
        addDiagnostic({
          path,
          line: parsePositiveInt(tsMatch[2]),
          column: parsePositiveInt(tsMatch[3]),
          severity: tsMatch[4] === 'warning' ? 'warning' : 'error',
          code: `TS${tsMatch[5]}`,
          message: tsMatch[6]?.trim() || 'TypeScript diagnostic.',
          source: 'typescript',
        })
      }
      continue
    }

    const rustHeaderMatch = trimmed.match(/^(error(?:\[[^\]]+\])?|warning)(?::\s*(.+))?$/)
    if (rustHeaderMatch) {
      const severity = rustHeaderMatch[1]?.startsWith('warning') ? 'warning' : 'error'
      const code = rustHeaderMatch[1]?.match(/\[([^\]]+)\]/)?.[1] ?? null
      rustDiagnostic = {
        severity,
        code,
        message: rustHeaderMatch[2]?.trim() || (severity === 'warning' ? 'Rust warning.' : 'Rust error.'),
      }
      continue
    }

    const rustLocationMatch = trimmed.match(/^-->\s+(.+?):(\d+):(\d+)$/)
    if (rustLocationMatch && rustDiagnostic) {
      const path = normalizeProblemPath(rustLocationMatch[1] ?? '', options.projectRoot)
      if (path) {
        addDiagnostic({
          path,
          line: parsePositiveInt(rustLocationMatch[2]),
          column: parsePositiveInt(rustLocationMatch[3]),
          severity: rustDiagnostic.severity,
          code: rustDiagnostic.code,
          message: rustDiagnostic.message,
          source: 'rustc',
        })
      }
      continue
    }

    if (looksLikeStandalonePath(trimmed, options.projectRoot)) {
      eslintFile = trimmed
      continue
    }

    const eslintStylishMatch = line.match(
      /^\s*(\d+):(\d+)\s+(error|warning)\s+(.+?)(?:\s{2,}([@\w/-]+(?:\/[\w-]+)?))?\s*$/,
    )
    if (eslintStylishMatch && eslintFile) {
      const path = normalizeProblemPath(eslintFile, options.projectRoot)
      if (path) {
        addDiagnostic({
          path,
          line: parsePositiveInt(eslintStylishMatch[1]),
          column: parsePositiveInt(eslintStylishMatch[2]),
          severity: eslintStylishMatch[3] === 'warning' ? 'warning' : 'error',
          code: eslintStylishMatch[5]?.trim() || null,
          message: eslintStylishMatch[4]?.trim() || 'ESLint diagnostic.',
          source: 'eslint',
        })
      }
      continue
    }

    const eslintUnixMatch = trimmed.match(
      /^(.+?):(\d+):(\d+):\s+(.+?)\s+\[(Error|Warning)\/([^\]]+)\]$/,
    )
    if (eslintUnixMatch) {
      const path = normalizeProblemPath(eslintUnixMatch[1] ?? '', options.projectRoot)
      if (path) {
        addDiagnostic({
          path,
          line: parsePositiveInt(eslintUnixMatch[2]),
          column: parsePositiveInt(eslintUnixMatch[3]),
          severity: eslintUnixMatch[5] === 'Warning' ? 'warning' : 'error',
          code: eslintUnixMatch[6]?.trim() || null,
          message: eslintUnixMatch[4]?.trim() || 'ESLint diagnostic.',
          source: 'eslint',
        })
      }
      continue
    }

    const pythonFrameMatch = trimmed.match(/^File "([^"]+)", line (\d+)(?:, in .*)?$/)
    if (pythonFrameMatch) {
      pythonFrame = {
        path: pythonFrameMatch[1] ?? '',
        line: parsePositiveInt(pythonFrameMatch[2]) ?? 1,
      }
      continue
    }

    if (pythonFrame && /^[A-Za-z_][\w.]*Error:/.test(trimmed)) {
      const path = normalizeProblemPath(pythonFrame.path, options.projectRoot)
      if (path) {
        const [code, ...rest] = trimmed.split(':')
        addDiagnostic({
          path,
          line: pythonFrame.line,
          column: 1,
          severity: 'error',
          code: code || null,
          message: rest.join(':').trim() || trimmed,
          source: 'python',
        })
      }
      pythonFrame = null
      continue
    }

    const typedGenericMatch = trimmed.match(
      /^(.+?):(\d+):(?:(\d+):)?\s+(error|warning|note|info|E\d+|F\d+|W\d+|[A-Z]\d+):\s+(.+)$/,
    )
    if (typedGenericMatch && looksLikeSourcePath(typedGenericMatch[1] ?? '')) {
      const path = normalizeProblemPath(typedGenericMatch[1] ?? '', options.projectRoot)
      if (path) {
        const marker = typedGenericMatch[4] ?? ''
        addDiagnostic({
          path,
          line: parsePositiveInt(typedGenericMatch[2]),
          column: parsePositiveInt(typedGenericMatch[3]) ?? 1,
          severity: marker === 'warning' || marker.startsWith('W') ? 'warning' : marker === 'note' || marker === 'info' ? 'info' : 'error',
          code: /^[A-Z]\d+/.test(marker) ? marker : null,
          message: typedGenericMatch[5]?.trim() || 'Task diagnostic.',
          source: inferSourceFromPath(path, marker),
        })
      }
      continue
    }

    const genericMatch = trimmed.match(/^(.+?):(\d+):(\d+):\s+(.+)$/)
    if (genericMatch && looksLikeSourcePath(genericMatch[1] ?? '')) {
      const path = normalizeProblemPath(genericMatch[1] ?? '', options.projectRoot)
      if (path) {
        addDiagnostic({
          path,
          line: parsePositiveInt(genericMatch[2]),
          column: parsePositiveInt(genericMatch[3]),
          severity: 'error',
          code: null,
          message: genericMatch[4]?.trim() || 'Task diagnostic.',
          source: inferSourceFromPath(path, ''),
        })
      }
    }
  }

  return diagnostics
}

export function stripTerminalControlSequences(value: string): string {
  return value
    .replace(/\x1b\][^\x07]*(?:\x07|\x1b\\)/g, '')
    .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, '')
    .replace(/\r\n/g, '\n')
    .replace(/\r/g, '\n')
}

function normalizeProblemPath(rawPath: string, projectRoot?: string | null): string | null {
  let path = rawPath.trim().replace(/^["']|["']$/g, '').replace(/\\/g, '/')
  if (path.startsWith('file://')) {
    path = path.slice('file://'.length)
  }
  const root = projectRoot?.trim().replace(/\\/g, '/').replace(/\/+$/, '')
  if (root && path === root) return null
  if (root && path.startsWith(`${root}/`)) {
    path = path.slice(root.length + 1)
  } else if (path.startsWith('/')) {
    return null
  }
  path = path.replace(/^\.\//, '').replace(/^\/+/, '')
  if (!path || path === '.') return null
  return `/${path}`
}

function parsePositiveInt(value: string | undefined): number | null {
  if (!value) return null
  const parsed = Number.parseInt(value, 10)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null
}

function looksLikeStandalonePath(value: string, projectRoot?: string | null): boolean {
  if (value.includes(':')) return false
  return looksLikeSourcePath(value) || (!!projectRoot && value.startsWith(projectRoot))
}

function looksLikeSourcePath(value: string): boolean {
  const clean = value.trim().replace(/^["']|["']$/g, '').split(/[?#]/)[0] ?? ''
  const basename = clean.split(/[\\/]/).pop() ?? ''
  const extension = basename.includes('.') ? basename.split('.').pop()?.toLowerCase() : null
  return !!extension && SOURCE_PATH_EXTENSIONS.has(extension)
}

function inferSourceFromPath(path: string, marker: string): string {
  if (marker.startsWith('E') || marker.startsWith('F') || marker.startsWith('W')) return 'python'
  const extension = path.split('.').pop()?.toLowerCase()
  if (extension === 'go') return 'go'
  if (extension === 'py' || extension === 'pyi') return 'python'
  if (extension === 'rs') return 'rustc'
  return 'task'
}
