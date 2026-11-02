import * as React from "react"
import { cn } from "cn"
import {
  CircleCheckIcon,
  CircleXIcon,
  InfoIcon,
  TriangleAlertIcon,
} from "lucide-react"

export type NoticeVariant = "info" | "success" | "warning" | "error"

const look = {
  info: { icon: InfoIcon, className: "border-info bg-info/10" },
  success: { icon: CircleCheckIcon, className: "border-success bg-success/10" },
  warning: {
    icon: TriangleAlertIcon,
    className: "border-warning bg-warning/15",
  },
  error: {
    icon: CircleXIcon,
    className: "border-destructive bg-destructive/10",
  },
} satisfies Record<
  NoticeVariant,
  { icon: React.ElementType; className: string }
>

/**
 * A message about the state of the screen. The icon and the words carry the meaning;
 * colour only backs them up. `error` is announced at once (role="alert"), the others
 * politely (role="status"). Pass `live={false}` when the page keeps its own live region
 * around it (a notice that replaces another inside one region is then still announced).
 */
function Notice({
  variant = "info",
  title,
  children,
  action,
  live = true,
  className,
}: {
  variant?: NoticeVariant
  title: React.ReactNode
  /** Optional detail under the title: what happened, what to do. */
  children?: React.ReactNode
  /** Optional button or link, e.g. "Try again". */
  action?: React.ReactNode
  live?: boolean
  className?: string
}) {
  const { icon: Icon, className: tone } = look[variant]
  return (
    <div
      role={live ? (variant === "error" ? "alert" : "status") : undefined}
      data-slot="notice"
      data-variant={variant}
      className={cn(
        "flex gap-3 rounded-lg border border-l-4 p-4 text-foreground",
        tone,
        className
      )}
    >
      <Icon aria-hidden="true" className="mt-0.5 size-5 shrink-0" />
      <div className="flex min-w-0 flex-col items-start gap-2">
        <p className="font-medium">{title}</p>
        {children ? <div className="text-sm">{children}</div> : null}
        {action}
      </div>
    </div>
  )
}

export { Notice }
