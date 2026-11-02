import { EmptyState as SharedEmptyState } from '@genesy/ui/components/empty-state'

/**
 * Zero rows is a legitimate answer, not an error.
 *
 * RLS returning nothing means "you may not see this". Render this, never an
 * error, and never retry — there is nothing to retry.
 *
 * The shared EmptyState: no live region, no retry, no tone — an empty result
 * must not look like a failure.
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
    <div data-testid="empty-state">
      <SharedEmptyState title={title} description={detail} action={action} />
    </div>
  )
}
