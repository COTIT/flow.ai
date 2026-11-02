import * as React from "react"

/** Waiting for data. (Notices and empty states come from @genesy/ui.) */
export function Loading({ children }: { children: React.ReactNode }) {
  return (
    <p role="status" aria-busy className="text-sm text-muted-foreground">
      {children}
    </p>
  )
}
