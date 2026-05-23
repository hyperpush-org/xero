import { FlaskConical, Info } from "lucide-react"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

export function BetaStep() {
  return (
    <div>
      <div>
        <h2 className="flex items-center gap-2.5 text-2xl font-semibold tracking-tight text-foreground">
          <span
            aria-hidden
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border border-primary/40 bg-primary/10 text-primary"
          >
            <FlaskConical className="h-4 w-4" />
          </span>
          Early beta
        </h2>
        <p className="mt-2 text-[13px] leading-relaxed text-muted-foreground">
          Xero is still early. You may hit rough edges or unexpected issues.
        </p>
      </div>

      <Alert className="mt-7 animate-in border-primary/40 bg-primary/[0.04] py-3 text-foreground fade-in-0 slide-in-from-bottom-1 motion-enter [animation-delay:60ms] [animation-fill-mode:both] [&>svg]:text-primary">
        <Info className="h-4 w-4" />
        <AlertTitle className="text-[13px]">Thanks for trying Xero early.</AlertTitle>
        <AlertDescription className="text-[12px] leading-relaxed text-muted-foreground">
          We are improving quickly, and your feedback helps shape what comes next.
        </AlertDescription>
      </Alert>
    </div>
  )
}
