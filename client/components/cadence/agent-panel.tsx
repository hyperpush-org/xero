"use client"

import { useState, useRef, useEffect } from "react"
import { AgentMessage, AgentTask } from "@/app/page"
import { cn } from "@/lib/utils"
import { 
  Send,
  FileText,
  Pencil,
  Terminal,
  Search,
  Brain,
  ChevronDown,
  ChevronRight,
  CheckCircle2,
  Loader2,
  AlertCircle,
  Clock,
  Paperclip,
  Sparkles,
  GitBranch
} from "lucide-react"

interface AgentPanelProps {
  messages: AgentMessage[]
  onSendMessage: (content: string) => void
}

const taskIcons: Record<AgentTask["type"], React.ElementType> = {
  read: FileText,
  write: Pencil,
  execute: Terminal,
  search: Search,
  think: Brain
}

const taskColors: Record<AgentTask["type"], string> = {
  read: "text-chart-1",
  write: "text-accent",
  execute: "text-chart-4",
  search: "text-chart-2",
  think: "text-muted-foreground"
}

export function AgentPanel({ messages, onSendMessage }: AgentPanelProps) {
  const [input, setInput] = useState("")
  const messagesEndRef = useRef<HTMLDivElement>(null)
  
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" })
  }, [messages])
  
  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (input.trim()) {
      onSendMessage(input.trim())
      setInput("")
    }
  }

  return (
    <div className="flex-1 flex flex-col bg-background">
      {/* Agent status bar */}
      <div className="px-4 py-2 border-b border-border bg-card/50">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-2">
              <div className="w-2 h-2 rounded-full bg-success animate-pulse" />
              <span className="text-sm font-medium text-foreground">Agent Runtime</span>
            </div>
            <div className="h-4 w-px bg-border" />
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <GitBranch className="w-3.5 h-3.5" />
              <span className="font-mono">feature/auth-middleware</span>
            </div>
          </div>
          <div className="flex items-center gap-4 text-xs text-muted-foreground">
            <div className="flex items-center gap-1.5">
              <Clock className="w-3.5 h-3.5" />
              <span>Session: 42m</span>
            </div>
            <div className="flex items-center gap-1.5">
              <Sparkles className="w-3.5 h-3.5 text-primary" />
              <span>23 tasks</span>
            </div>
          </div>
        </div>
      </div>
      
      {/* Messages area */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {messages.map((message) => (
          <MessageBubble key={message.id} message={message} />
        ))}
        <div ref={messagesEndRef} />
      </div>
      
      {/* Input area */}
      <div className="p-4 border-t border-border bg-card/30">
        <form onSubmit={handleSubmit}>
          <div className="relative">
            <textarea
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault()
                  handleSubmit(e)
                }
              }}
              placeholder="Describe what you want to build or ask for help..."
              className="w-full min-h-[80px] max-h-[200px] px-4 py-3 pr-24 bg-input border border-border rounded-lg resize-none text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              rows={3}
            />
            <div className="absolute bottom-3 right-3 flex items-center gap-2">
              <button
                type="button"
                className="p-2 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
              >
                <Paperclip className="w-4 h-4" />
              </button>
              <button
                type="submit"
                disabled={!input.trim()}
                className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <Send className="w-4 h-4" />
                <span>Send</span>
              </button>
            </div>
          </div>
          <div className="flex items-center gap-4 mt-2 text-xs text-muted-foreground">
            <span>Press <kbd className="px-1.5 py-0.5 rounded bg-muted font-mono">Enter</kbd> to send</span>
            <span><kbd className="px-1.5 py-0.5 rounded bg-muted font-mono">Shift+Enter</kbd> for new line</span>
          </div>
        </form>
      </div>
    </div>
  )
}

function MessageBubble({ message }: { message: AgentMessage }) {
  const [expanded, setExpanded] = useState(true)
  
  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div className="max-w-[80%] bg-primary text-primary-foreground rounded-lg px-4 py-3">
          <p className="text-sm whitespace-pre-wrap">{message.content}</p>
          <div className="text-[10px] opacity-70 mt-2">{message.timestamp}</div>
        </div>
      </div>
    )
  }
  
  return (
    <div className="flex gap-3">
      <div className="w-8 h-8 rounded-full bg-secondary flex items-center justify-center shrink-0">
        <Sparkles className="w-4 h-4 text-primary" />
      </div>
      <div className="flex-1 min-w-0">
        <div className="bg-card border border-border rounded-lg overflow-hidden">
          <div className="px-4 py-3">
            <p className="text-sm text-card-foreground whitespace-pre-wrap">{message.content}</p>
          </div>
          
          {message.tasks && message.tasks.length > 0 && (
            <div className="border-t border-border">
              <button
                onClick={() => setExpanded(!expanded)}
                className="w-full flex items-center gap-2 px-4 py-2 text-xs text-muted-foreground hover:bg-muted/50 transition-colors"
              >
                {expanded ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
                <span>{message.tasks.length} tool calls</span>
                <TaskStatusSummary tasks={message.tasks} />
              </button>
              
              {expanded && (
                <div className="px-4 pb-3 space-y-1">
                  {message.tasks.map((task) => (
                    <TaskItem key={task.id} task={task} />
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
        <div className="text-[10px] text-muted-foreground mt-1 ml-1">{message.timestamp}</div>
      </div>
    </div>
  )
}

function TaskStatusSummary({ tasks }: { tasks: AgentTask[] }) {
  const complete = tasks.filter(t => t.status === "complete").length
  const running = tasks.filter(t => t.status === "running").length
  const error = tasks.filter(t => t.status === "error").length
  
  return (
    <div className="flex items-center gap-2 ml-auto">
      {complete > 0 && (
        <span className="flex items-center gap-1 text-success">
          <CheckCircle2 className="w-3 h-3" />
          {complete}
        </span>
      )}
      {running > 0 && (
        <span className="flex items-center gap-1 text-primary">
          <Loader2 className="w-3 h-3 animate-spin" />
          {running}
        </span>
      )}
      {error > 0 && (
        <span className="flex items-center gap-1 text-destructive">
          <AlertCircle className="w-3 h-3" />
          {error}
        </span>
      )}
    </div>
  )
}

function TaskItem({ task }: { task: AgentTask }) {
  const [showDetails, setShowDetails] = useState(false)
  const Icon = taskIcons[task.type]
  
  return (
    <div className="group">
      <button
        onClick={() => task.details && setShowDetails(!showDetails)}
        className={cn(
          "w-full flex items-center gap-2 px-3 py-2 rounded-md text-left transition-colors",
          task.details ? "hover:bg-muted/50 cursor-pointer" : "cursor-default",
          showDetails && "bg-muted/50"
        )}
      >
        <Icon className={cn("w-4 h-4 shrink-0", taskColors[task.type])} />
        <span className="flex-1 text-sm text-foreground font-mono truncate">
          {task.description}
        </span>
        <TaskStatus status={task.status} />
      </button>
      
      {showDetails && task.details && (
        <div className="ml-9 mr-3 mt-1 mb-2 px-3 py-2 rounded-md bg-muted/30 border border-border">
          <p className="text-xs text-muted-foreground">{task.details}</p>
        </div>
      )}
    </div>
  )
}

function TaskStatus({ status }: { status: AgentTask["status"] }) {
  if (status === "complete") {
    return <CheckCircle2 className="w-4 h-4 text-success shrink-0" />
  }
  if (status === "running") {
    return <Loader2 className="w-4 h-4 text-primary animate-spin shrink-0" />
  }
  if (status === "error") {
    return <AlertCircle className="w-4 h-4 text-destructive shrink-0" />
  }
  return <Clock className="w-4 h-4 text-muted-foreground shrink-0" />
}
