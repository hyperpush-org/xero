import { cn } from "@/lib/utils"

interface StepIndicatorProps {
  total: number
  currentIndex: number
}

export function StepIndicator({ total, currentIndex }: StepIndicatorProps) {
  return (
    <div
      className="flex items-center gap-1.5"
      role="group"
      aria-label={`Step ${currentIndex + 1} of ${total}`}
    >
      {Array.from({ length: total }).map((_, index) => {
        const isCurrent = index === currentIndex
        const isDone = index < currentIndex
        return (
          <span
            key={index}
            aria-current={isCurrent ? "step" : undefined}
            className={cn(
              "h-1 rounded-full transition-[width,background-color] motion-standard",
              isCurrent
                ? "w-6 bg-primary"
                : isDone
                  ? "w-1.5 bg-primary/50"
                  : "w-1.5 bg-border",
            )}
          />
        )
      })}
      <span className="ml-1.5 text-[10px] font-medium tabular-nums text-muted-foreground">
        {currentIndex + 1}
        <span className="text-muted-foreground/50">/{total}</span>
      </span>
    </div>
  )
}
