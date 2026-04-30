import { FileCode } from 'lucide-react'

interface EditorEmptyStateProps {
  loadingPath: string | null
  projectLabel: string
}

export function LoadingState({ path }: { path: string }) {
  return (
    <div className="flex flex-1 items-center justify-center bg-background">
      <div className="text-center text-[12px] text-muted-foreground">Opening {path.split('/').pop() ?? path}…</div>
    </div>
  )
}

export function EditorEmptyState({ loadingPath, projectLabel }: EditorEmptyStateProps) {
  return (
    <div className="flex flex-1 items-center justify-center bg-background">
      <div className="flex max-w-sm flex-col items-center gap-4 text-center">
        <div className="flex h-12 w-12 items-center justify-center rounded-xl border border-border bg-card">
          <FileCode className="h-6 w-6 text-muted-foreground" />
        </div>
        <div>
          <p className="text-[14px] font-medium text-foreground">
            {loadingPath ? 'Opening file…' : 'Select a file to start editing'}
          </p>
          <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
            {loadingPath
              ? `Xero is loading ${loadingPath.split('/').pop() ?? loadingPath} from ${projectLabel}.`
              : `Pick a file from the selected project explorer. Edits save directly back to ${projectLabel}.`}
          </p>
        </div>
        <div className="flex items-center gap-2 text-[10px] text-muted-foreground/70">
          <Shortcut keys={['⌘', 'S']} label="Save" />
          <span>·</span>
          <Shortcut keys={['⌘', 'W']} label="Close tab" />
        </div>
      </div>
    </div>
  )
}

function Shortcut({ keys, label }: { keys: string[]; label: string }) {
  return (
    <span className="flex items-center gap-1">
      {keys.map((key, index) => (
        <kbd
          key={`${key}-${index}`}
          className="rounded border border-border bg-card px-1.5 py-0.5 font-mono text-[10px] text-foreground/70"
        >
          {key}
        </kbd>
      ))}
      <span>{label}</span>
    </span>
  )
}
