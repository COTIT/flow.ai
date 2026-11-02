import * as React from "react"
import { cn } from "cn"
import { InboxIcon } from "lucide-react"

/** Nothing to show yet: say what is empty, why, and offer the next step. Not a live region. */
function EmptyState({
  title,
  description,
  action,
  icon = <InboxIcon aria-hidden="true" className="size-8" />,
  className,
}: {
  title: React.ReactNode
  /** One line on why it is empty. */
  description?: React.ReactNode
  /** Optional button or link, e.g. "Add product", "Browse the shop". */
  action?: React.ReactNode
  icon?: React.ReactNode
  className?: string
}) {
  return (
    <div
      className={cn(
        "flex flex-col items-center gap-3 rounded-lg border border-dashed px-4 py-10 text-center",
        className
      )}
    >
      <span className="text-muted-foreground">{icon}</span>
      <p className="font-medium">{title}</p>
      {description ? (
        <p className="max-w-md text-sm text-muted-foreground">{description}</p>
      ) : null}
      {action}
    </div>
  )
}

export { EmptyState }
