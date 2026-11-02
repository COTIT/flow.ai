import * as React from "react"

import { cn } from "@genesy/ui/lib/utils"

const tones = {
  info: "border-border bg-muted/50",
  success: "border-teal/40 bg-teal/10",
  warning: "border-gold bg-gold/15",
  error: "border-destructive/40 bg-destructive/10 text-destructive",
} as const

/**
 * shortcut: interim inline notice until design's shared Notice (packages/ui) lands; then this
 * file re-exports it. Errors are role=alert, the rest role=status, so screen readers hear them.
 */
export function Notice({
  tone,
  children,
  action,
}: {
  tone: keyof typeof tones
  children: React.ReactNode
  action?: React.ReactNode
}) {
  return (
    <div
      role={tone === "error" ? "alert" : "status"}
      className={cn(
        "flex flex-wrap items-center gap-3 rounded-lg border p-3 text-sm",
        tones[tone]
      )}
    >
      <div className="mr-auto">{children}</div>
      {action}
    </div>
  )
}
