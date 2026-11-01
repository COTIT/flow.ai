/**
 * Zero rows is a legitimate answer, not an error.
 *
 * RLS returning nothing means "you may not see this". Render this, never an
 * error, and never retry — there is nothing to retry.
 *
 * Deliberately carries no `role="alert"` and no retry control: an empty result
 * must not look like a failure. Sunken sand, a dashed ring, and not one red
 * thing anywhere — an empty state and an error state must not rhyme.
 */
export function EmptyState({
  title,
  detail,
  action,
}: {
  title: string
  detail?: string
  action?: React.ReactNode
}) {
  return (
    <div
      data-testid="empty-state"
      className="flex flex-col items-center gap-2 px-5 py-8 text-center"
      style={{
        background: 'var(--sand-2)',
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-card)',
      }}
    >
      <span
        data-mark="empty"
        aria-hidden
        style={{
          width: 22,
          height: 22,
          borderRadius: 'var(--radius-pill)',
          border: '2px dashed var(--ink-3)',
          boxSizing: 'border-box',
          flex: 'none',
        }}
      />
      <p style={{ fontSize: 16, lineHeight: '22px', fontWeight: 600, color: 'var(--ink)' }}>
        {title}
      </p>
      {detail && (
        <p
          className="max-w-prose"
          style={{ fontSize: 13, lineHeight: 1.5, color: 'var(--ink-2)', textWrap: 'pretty' }}
        >
          {detail}
        </p>
      )}
      {action && <div className="mt-1">{action}</div>}
    </div>
  )
}
