import type { ReactNode } from 'react'

/**
 * A label above its control, with an optional hint under it — the ops form
 * idiom from DemandListScreen, shared by the survey forms.
 */
export function Field({
  label,
  htmlFor,
  hint,
  children,
}: {
  label: string
  htmlFor: string
  hint?: string
  children: ReactNode
}) {
  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      <label
        className="block"
        htmlFor={htmlFor}
        style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}
      >
        {label}
      </label>
      {children}
      {hint && (
        <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
          {hint}
        </p>
      )}
    </div>
  )
}

/** A bordered white panel on the sand ground. Depth is a border. */
export function Panel({
  children,
  testId,
  id,
  className = '',
}: {
  children: ReactNode
  testId?: string
  id?: string
  className?: string
}) {
  return (
    <section
      id={id}
      data-testid={testId}
      className={`flex w-full flex-col gap-3 p-4 sm:p-[18px] ${className}`}
      style={{
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-card)',
        background: 'var(--paper)',
      }}
    >
      {children}
    </section>
  )
}

export function SectionTitle({ children }: { children: ReactNode }) {
  return (
    <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
      {children}
    </h2>
  )
}
