"use client"

import { useState } from "react"
import { Project } from "@/app/page"
import { cn } from "@/lib/utils"
import {
  FileCode,
  GitCommit,
  Package,
  AlertTriangle,
  CheckCircle2,
  XCircle,
  ChevronRight,
  ChevronDown,
  Play,
  Pause,
  RotateCcw,
  ExternalLink,
  Copy,
  Terminal,
  FileText,
  Folder,
  Plus,
  Minus,
  Activity
} from "lucide-react"

interface ExecutionPanelProps {
  project: Project
}

interface DiffFile {
  path: string
  additions: number
  deletions: number
  status: "added" | "modified" | "deleted"
}

interface Artifact {
  id: string
  name: string
  type: "file" | "log" | "report"
  size: string
  timestamp: string
}

interface Diagnostic {
  id: string
  type: "error" | "warning" | "info"
  message: string
  file?: string
  line?: number
}

const mockDiffFiles: DiffFile[] = [
  { path: "src/middleware/auth.ts", additions: 142, deletions: 0, status: "added" },
  { path: "src/middleware/index.ts", additions: 8, deletions: 2, status: "modified" },
  { path: "src/config/jwt.ts", additions: 35, deletions: 0, status: "added" },
  { path: "src/types/auth.ts", additions: 28, deletions: 0, status: "added" },
  { path: "package.json", additions: 3, deletions: 1, status: "modified" }
]

const mockArtifacts: Artifact[] = [
  { id: "1", name: "auth.ts", type: "file", size: "4.2 KB", timestamp: "10:33 AM" },
  { id: "2", name: "build.log", type: "log", size: "12 KB", timestamp: "10:32 AM" },
  { id: "3", name: "coverage-report.html", type: "report", size: "156 KB", timestamp: "10:30 AM" }
]

const mockDiagnostics: Diagnostic[] = [
  { id: "1", type: "warning", message: "Consider using a more specific type instead of 'any'", file: "src/middleware/auth.ts", line: 45 },
  { id: "2", type: "info", message: "JWT secret should be loaded from environment variables", file: "src/config/jwt.ts", line: 12 },
  { id: "3", type: "error", message: "Missing required dependency: jsonwebtoken", file: "package.json" }
]

const mockDiffContent = `@@ -1,6 +1,14 @@
 import { NextFunction, Request, Response } from 'express';
+import { verifyToken, refreshAccessToken } from './auth';
+import { JWTPayload } from '../types/auth';
 
 export function setupMiddleware(app: Express) {
   app.use(cors());
   app.use(helmet());
+  app.use('/api', authenticateRequest);
+  app.use('/api', validateTokenExpiry);
 }
+
+export { authenticateRequest, validateTokenExpiry } from './auth';`

export function ExecutionPanel({ project }: ExecutionPanelProps) {
  const [activeTab, setActiveTab] = useState<"diffs" | "artifacts" | "diagnostics">("diffs")
  const [selectedFile, setSelectedFile] = useState<string | null>(mockDiffFiles[0].path)
  const [expandedDiffs, setExpandedDiffs] = useState<Set<string>>(new Set([mockDiffFiles[0].path]))

  const toggleDiff = (path: string) => {
    const newExpanded = new Set(expandedDiffs)
    if (newExpanded.has(path)) {
      newExpanded.delete(path)
    } else {
      newExpanded.add(path)
    }
    setExpandedDiffs(newExpanded)
    setSelectedFile(path)
  }

  return (
    <div className="flex-1 flex bg-background overflow-hidden">
      {/* Left panel - file tree / list */}
      <div className="w-80 border-r border-border flex flex-col shrink-0">
        {/* Tabs */}
        <div className="flex border-b border-border bg-card/50">
          <TabButton
            active={activeTab === "diffs"}
            onClick={() => setActiveTab("diffs")}
            icon={GitCommit}
            label="Changes"
            count={mockDiffFiles.length}
          />
          <TabButton
            active={activeTab === "artifacts"}
            onClick={() => setActiveTab("artifacts")}
            icon={Package}
            label="Artifacts"
            count={mockArtifacts.length}
          />
          <TabButton
            active={activeTab === "diagnostics"}
            onClick={() => setActiveTab("diagnostics")}
            icon={AlertTriangle}
            label="Issues"
            count={mockDiagnostics.length}
          />
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto">
          {activeTab === "diffs" && (
            <DiffFileList
              files={mockDiffFiles}
              expandedFiles={expandedDiffs}
              onToggle={toggleDiff}
            />
          )}
          {activeTab === "artifacts" && (
            <ArtifactList artifacts={mockArtifacts} />
          )}
          {activeTab === "diagnostics" && (
            <DiagnosticsList diagnostics={mockDiagnostics} />
          )}
        </div>

        {/* Actions bar */}
        <div className="p-3 border-t border-border bg-card/30">
          <div className="flex items-center gap-2">
            <button className="flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90 transition-colors">
              <CheckCircle2 className="w-4 h-4" />
              <span>Approve Changes</span>
            </button>
            <button className="p-2 rounded-md border border-border hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">
              <RotateCcw className="w-4 h-4" />
            </button>
          </div>
        </div>
      </div>

      {/* Right panel - diff viewer / detail */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {/* File header */}
        {selectedFile && (
          <div className="px-4 py-2 border-b border-border bg-card/50 flex items-center justify-between">
            <div className="flex items-center gap-2">
              <FileCode className="w-4 h-4 text-muted-foreground" />
              <span className="font-mono text-sm text-foreground">{selectedFile}</span>
            </div>
            <div className="flex items-center gap-2">
              <button className="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">
                <Copy className="w-4 h-4" />
              </button>
              <button className="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">
                <ExternalLink className="w-4 h-4" />
              </button>
            </div>
          </div>
        )}

        {/* Diff content */}
        <div className="flex-1 overflow-auto bg-card/20">
          <DiffViewer content={mockDiffContent} />
        </div>

        {/* Live execution footer */}
        <div className="border-t border-border bg-sidebar">
          <LiveExecutionBar />
        </div>
      </div>
    </div>
  )
}

interface TabButtonProps {
  active: boolean
  onClick: () => void
  icon: React.ElementType
  label: string
  count: number
}

function TabButton({ active, onClick, icon: Icon, label, count }: TabButtonProps) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex-1 flex items-center justify-center gap-2 px-3 py-2.5 text-sm transition-colors border-b-2",
        active
          ? "border-primary text-foreground"
          : "border-transparent text-muted-foreground hover:text-foreground"
      )}
    >
      <Icon className="w-4 h-4" />
      <span>{label}</span>
      <span className={cn(
        "px-1.5 py-0.5 rounded-full text-[10px] font-medium",
        active ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground"
      )}>
        {count}
      </span>
    </button>
  )
}

function DiffFileList({ 
  files, 
  expandedFiles, 
  onToggle 
}: { 
  files: DiffFile[]
  expandedFiles: Set<string>
  onToggle: (path: string) => void
}) {
  return (
    <div className="p-2">
      {files.map((file) => (
        <button
          key={file.path}
          onClick={() => onToggle(file.path)}
          className={cn(
            "w-full flex items-center gap-2 px-3 py-2 rounded-md text-left transition-colors",
            expandedFiles.has(file.path) ? "bg-muted" : "hover:bg-muted/50"
          )}
        >
          {expandedFiles.has(file.path) ? (
            <ChevronDown className="w-4 h-4 text-muted-foreground shrink-0" />
          ) : (
            <ChevronRight className="w-4 h-4 text-muted-foreground shrink-0" />
          )}
          <FileStatusIcon status={file.status} />
          <span className="flex-1 font-mono text-sm text-foreground truncate">
            {file.path.split('/').pop()}
          </span>
          <div className="flex items-center gap-1.5 text-xs">
            <span className="text-success flex items-center gap-0.5">
              <Plus className="w-3 h-3" />
              {file.additions}
            </span>
            <span className="text-destructive flex items-center gap-0.5">
              <Minus className="w-3 h-3" />
              {file.deletions}
            </span>
          </div>
        </button>
      ))}
    </div>
  )
}

function FileStatusIcon({ status }: { status: DiffFile["status"] }) {
  if (status === "added") {
    return <Plus className="w-4 h-4 text-success shrink-0" />
  }
  if (status === "deleted") {
    return <Minus className="w-4 h-4 text-destructive shrink-0" />
  }
  return <FileCode className="w-4 h-4 text-accent shrink-0" />
}

function ArtifactList({ artifacts }: { artifacts: Artifact[] }) {
  const getIcon = (type: Artifact["type"]) => {
    switch (type) {
      case "file": return FileCode
      case "log": return Terminal
      case "report": return FileText
    }
  }

  return (
    <div className="p-2 space-y-1">
      {artifacts.map((artifact) => {
        const Icon = getIcon(artifact.type)
        return (
          <button
            key={artifact.id}
            className="w-full flex items-center gap-3 px-3 py-2 rounded-md hover:bg-muted/50 transition-colors text-left"
          >
            <Icon className="w-4 h-4 text-muted-foreground shrink-0" />
            <div className="flex-1 min-w-0">
              <div className="font-mono text-sm text-foreground truncate">{artifact.name}</div>
              <div className="text-xs text-muted-foreground">{artifact.size}</div>
            </div>
            <span className="text-xs text-muted-foreground">{artifact.timestamp}</span>
          </button>
        )
      })}
    </div>
  )
}

function DiagnosticsList({ diagnostics }: { diagnostics: Diagnostic[] }) {
  const getIcon = (type: Diagnostic["type"]) => {
    switch (type) {
      case "error": return XCircle
      case "warning": return AlertTriangle
      case "info": return Activity
    }
  }

  const getColor = (type: Diagnostic["type"]) => {
    switch (type) {
      case "error": return "text-destructive"
      case "warning": return "text-accent"
      case "info": return "text-primary"
    }
  }

  return (
    <div className="p-2 space-y-1">
      {diagnostics.map((diagnostic) => {
        const Icon = getIcon(diagnostic.type)
        return (
          <button
            key={diagnostic.id}
            className="w-full flex items-start gap-3 px-3 py-2 rounded-md hover:bg-muted/50 transition-colors text-left"
          >
            <Icon className={cn("w-4 h-4 shrink-0 mt-0.5", getColor(diagnostic.type))} />
            <div className="flex-1 min-w-0">
              <div className="text-sm text-foreground">{diagnostic.message}</div>
              {diagnostic.file && (
                <div className="text-xs text-muted-foreground font-mono mt-0.5">
                  {diagnostic.file}{diagnostic.line && `:${diagnostic.line}`}
                </div>
              )}
            </div>
          </button>
        )
      })}
    </div>
  )
}

function DiffViewer({ content }: { content: string }) {
  const lines = content.split('\n')

  return (
    <div className="font-mono text-sm">
      {lines.map((line, index) => {
        let bgClass = ""
        let textClass = "text-foreground"
        let lineType = ""

        if (line.startsWith('+') && !line.startsWith('+++')) {
          bgClass = "bg-success/10"
          textClass = "text-success"
          lineType = "+"
        } else if (line.startsWith('-') && !line.startsWith('---')) {
          bgClass = "bg-destructive/10"
          textClass = "text-destructive"
          lineType = "-"
        } else if (line.startsWith('@@')) {
          bgClass = "bg-primary/10"
          textClass = "text-primary"
          lineType = "@"
        }

        return (
          <div key={index} className={cn("flex", bgClass)}>
            <div className="w-12 px-2 py-0.5 text-right text-muted-foreground border-r border-border shrink-0 select-none">
              {index + 1}
            </div>
            <div className="w-6 px-1 py-0.5 text-center text-muted-foreground border-r border-border shrink-0 select-none">
              {lineType}
            </div>
            <pre className={cn("flex-1 px-4 py-0.5 whitespace-pre", textClass)}>
              {line || " "}
            </pre>
          </div>
        )
      })}
    </div>
  )
}

function LiveExecutionBar() {
  const [isRunning, setIsRunning] = useState(true)

  return (
    <div className="px-4 py-2 flex items-center gap-4">
      <div className="flex items-center gap-2">
        <button
          onClick={() => setIsRunning(!isRunning)}
          className={cn(
            "p-1.5 rounded-md transition-colors",
            isRunning ? "bg-destructive/20 text-destructive hover:bg-destructive/30" : "bg-success/20 text-success hover:bg-success/30"
          )}
        >
          {isRunning ? <Pause className="w-4 h-4" /> : <Play className="w-4 h-4" />}
        </button>
        <div className="flex items-center gap-1.5">
          <div className={cn(
            "w-2 h-2 rounded-full",
            isRunning ? "bg-success animate-pulse" : "bg-muted-foreground"
          )} />
          <span className="text-sm text-foreground">
            {isRunning ? "Executing" : "Paused"}
          </span>
        </div>
      </div>

      <div className="h-4 w-px bg-border" />

      <div className="flex-1 flex items-center gap-4 text-xs text-muted-foreground">
        <span>Phase 2 of 4</span>
        <div className="flex-1 h-1.5 bg-muted rounded-full max-w-xs">
          <div className="h-full w-[45%] bg-primary rounded-full" />
        </div>
        <span>45%</span>
      </div>

      <div className="flex items-center gap-2 text-xs">
        <span className="text-muted-foreground">Est. remaining:</span>
        <span className="font-mono text-foreground">~8 min</span>
      </div>
    </div>
  )
}
