import { ArrowRight } from 'lucide-react'
import { Link } from '@tanstack/react-router'

/** The records an aggregate can drill into. */
export type DrillKind = 'person' | 'farm' | 'cycle' | 'request' | 'demand' | 'opportunity'

const ROUTE: Record<DrillKind, { to: string; param: string }> = {
  person: { to: '/officer/people/$personId', param: 'personId' },
  farm: { to: '/officer/farms/$farmId', param: 'farmId' },
  cycle: { to: '/officer/cycles/$cycleId', param: 'cycleId' },
  request: { to: '/ops/requests/$requestId', param: 'requestId' },
  demand: { to: '/ops/demand/$demandId', param: 'demandId' },
  opportunity: { to: '/ops/opportunities/$opportunityId', param: 'opportunityId' },
}

// The router's `to` is a typed union; this is the single place it meets a
// path built from a lookup table.
type LinkTo = Parameters<typeof Link>[0]['to']

/**
 * Turns an aggregate cell into a path to its rows — spec §9.1.
 *
 * No headline without a path back to the records under it: that is the
 * traceability claim, and a figure with no route to its rows cannot support
 * it. Where there is genuinely nothing behind a value, this renders plain text
 * rather than a link that goes nowhere.
 *
 * The **value itself** is the link — not a "view" affordance parked beside it —
 * so the thing the eye lands on is the thing that leads somewhere. A soft rule
 * under it goes solid on hover, and the arrow says which direction.
 */
export function DrillLink({
  kind,
  id,
  children,
}: {
  kind: DrillKind
  id: string | null | undefined
  children: React.ReactNode
}) {
  if (!id) return <>{children}</>

  const route = ROUTE[kind]
  return (
    <Link
      to={route.to as LinkTo}
      params={{ [route.param]: id } as never}
      data-testid="drill-link"
      className="inline-flex items-baseline gap-1 font-semibold hover:border-b-[var(--primary-ink)]"
      style={{
        color: 'var(--primary-ink)',
        borderBottom: '1.5px solid rgba(29, 112, 183, .4)',
      }}
    >
      {children}
      <ArrowRight aria-hidden size={15} strokeWidth={2.25} style={{ flex: 'none' }} />
    </Link>
  )
}
