import * as React from "react"

import { cn } from "@genesy/ui/lib/utils"

const tone = {
  info: "border-border text-foreground",
  success: "border-primary/40 text-foreground",
  warning: "border-amber-500/50 text-foreground",
  error: "border-destructive/50 text-destructive",
}

/**
 * One message about the screen's state. Errors are alerts; the rest are polite status.
 * shortcut: house markup until the shared Notice in packages/ui lands; swap here only.
 */
export function Notice({
  kind,
  children,
  className,
}: {
  kind: keyof typeof tone
  children: React.ReactNode
  className?: string
}) {
  return (
    <div
      role={kind === "error" ? "alert" : "status"}
      data-kind={kind}
      className={cn(
        "flex flex-col gap-1 rounded-lg border px-3 py-2 text-sm",
        tone[kind],
        className
      )}
    >
      {children}
    </div>
  )
}

/** Nothing to show yet, said plainly. shortcut: until the shared EmptyState lands. */
export function Empty({ children }: { children: React.ReactNode }) {
  return (
    <p role="status" className="py-4 text-center text-sm text-muted-foreground">
      {children}
    </p>
  )
}

/** Waiting for data. */
export function Loading({ children }: { children: React.ReactNode }) {
  return (
    <p role="status" aria-busy className="text-sm text-muted-foreground">
      {children}
    </p>
  )
}
