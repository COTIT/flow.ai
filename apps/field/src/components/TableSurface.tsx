import type { ReactNode } from 'react'

export function TableSurface({
  children,
  toolbar,
  className = '',
}: {
  children: ReactNode
  toolbar?: ReactNode
  className?: string
}) {
  return (
    <section
      className={`w-full overflow-hidden rounded-[var(--radius-card)] border border-rule bg-paper ${className}`}
    >
      {toolbar && (
        <div className="flex flex-wrap items-end justify-between gap-3 border-b border-rule bg-paper p-4">
          {toolbar}
        </div>
      )}
      {children}
    </section>
  )
}
