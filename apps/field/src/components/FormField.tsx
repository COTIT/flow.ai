import type { ReactNode } from 'react'

import { BangMark } from '@/components/marks'

/**
 * A label above its control, an optional hint, and the field's error under it
 * — the ops create-form idiom, shared by Demand, Villages and the catalogue.
 */
export function FormField({
  label,
  id,
  hint,
  error,
  errorTestId,
  children,
}: {
  label: string
  id: string
  hint?: string
  error?: string
  errorTestId?: string
  children: ReactNode
}) {
  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      <label
        className="block"
        htmlFor={id}
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
      {error && (
        <p
          data-testid={errorTestId}
          className="flex items-start gap-[7px] font-medium"
          style={{ fontSize: 13, color: 'var(--flag-ink)' }}
        >
          <BangMark />
          {error}
        </p>
      )}
    </div>
  )
}
