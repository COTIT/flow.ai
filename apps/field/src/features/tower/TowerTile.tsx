import type { CSSProperties, ReactNode } from 'react'
import { ArrowRight } from 'lucide-react'
import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

type LinkTo = Parameters<typeof Link>[0]['to']

/**
 * One Tower tile.
 *
 * Every tile that carries a figure also carries a way to the rows behind it:
 * spec 8.2 is explicit that a number which cannot be traced does not belong on
 * this screen.
 *
 * The tiles are deliberately not uniform. They sit in a wrapping flex row and
 * each declares the width it needs for what it carries — Production leads at
 * 430px, Data quality is the smallest at 230px — so they land two-up on a
 * desktop and stack cleanly on a phone with no media query at all. Every one
 * carries `min-width: 0`, which is what stops a long figure forcing the row
 * wider than the screen.
 */
export function TowerTile({
  id,
  testId,
  title,
  icon,
  note,
  drillTo,
  drillSearch,
  loading,
  basis,
  sunken = false,
  children,
}: {
  id: string
  /**
   * The tile's test id when it is not simply `tile-${id}`. The Tower screen
   * spells its five out: a guided tour points at them, and a tour can only be
   * checked against ids that are written in the source, not built at render.
   */
  testId?: string
  title: string
  icon?: ReactNode
  note?: string
  drillTo?: string
  drillSearch?: Record<string, string | undefined>
  /** The tile's own query is still running — QA #29. */
  loading?: boolean
  /** How much room this tile needs. `flex: 1 1 <basis>`. */
  basis?: string
  /** Sunken sand rather than paper: it reports on the records, not on a figure. */
  sunken?: boolean
  children: React.ReactNode
}) {
  const { t } = useTranslation()

  return (
    <section
      data-testid={testId ?? `tile-${id}`}
      aria-busy={loading || undefined}
      className="flex flex-col gap-3.5 p-5"
      style={{
        flex: `1 1 ${basis ?? '320px'}`,
        minWidth: 0,
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-frame)',
        background: sunken ? 'var(--sand-2)' : 'var(--paper)',
      }}
    >
      <header className="flex flex-col gap-1">
        <div className="flex flex-wrap items-baseline justify-between gap-2.5">
          <h2 className="type-section inline-flex items-center gap-2" style={{ color: 'var(--ink-3)' }}>
            {icon}
            {title}
          </h2>
          {/* QA #29: the link used to render immediately while the tile was
              still loading, so a click landed on a drill-down whose own query
              had not started — from a figure nobody had seen. The label stays
              in place as plain text so the header does not jump when the
              figure arrives. */}
          {drillTo &&
            (loading ? (
              <span
                data-testid="tile-drill"
                style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-3)' }}
              >
                {t('tower.drill')}
              </span>
            ) : (
              <Link
                to={drillTo as LinkTo}
                search={drillSearch as never}
                data-testid="tile-drill"
                className="inline-flex items-center gap-1.5 font-semibold hover:border-b-[var(--primary-ink)]"
                style={{
                  fontSize: 13,
                  color: 'var(--primary-ink)',
                  borderBottom: '1.5px solid rgba(29, 112, 183, .4)',
                }}
              >
                {t('tower.drill')}
                <ArrowRight aria-hidden size={15} strokeWidth={2.5} style={{ flex: 'none' }} />
              </Link>
            ))}
        </div>
        {note && (
          <p className="type-note" style={{ color: 'var(--ink-3)' }}>
            {note}
          </p>
        )}
      </header>
      {children}
    </section>
  )
}

/**
 * The one figure a tile is built around.
 *
 * A pill — the capacity basis — belongs INSIDE this block rather than beside
 * it: "planned, not measured" is part of what the number means, and a layout
 * that can separate them will eventually separate them.
 */
export function Headline({
  label,
  icon,
  value,
  pill,
  note,
  testId,
}: {
  label: string
  icon?: ReactNode
  value: string
  pill?: ReactNode
  note?: string
  testId?: string
}) {
  return (
    <div data-testid={testId} className="flex flex-col gap-1">
      <p
        className="inline-flex items-center gap-[7px]"
        style={{ fontSize: 13, color: 'var(--ink-2)' }}
      >
        {icon}
        {label}
      </p>
      <p className="type-display tabular flex flex-wrap items-baseline gap-2.5">
        {value}
        {pill}
      </p>
      {note && (
        <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
          {note}
        </p>
      )}
    </div>
  )
}

/**
 * A labelled figure inside a tile. Figures stack, so they align.
 *
 * A grid track rather than `justify-between` on a flex row: the label has to be
 * able to wrap to two lines while the figure stays pinned to the right edge,
 * which is what makes the Kiswahili build survive.
 */
export function Figure({
  label,
  value,
  testId,
  note,
  icon,
  size = 15,
}: {
  label: string
  value: string
  testId?: string
  note?: string
  icon?: ReactNode
  size?: number
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <div className="grid items-baseline gap-3" style={{ gridTemplateColumns: 'minmax(0, 1fr) auto' }}>
        <span
          className="inline-flex items-center gap-[7px]"
          style={{ fontSize: 13, color: 'var(--ink-2)' }}
        >
          {icon}
          {label}
        </span>
        <span data-testid={testId} className="tabular font-semibold" style={{ fontSize: size }}>
          {value}
        </span>
      </div>
      {note && (
        <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
          {note}
        </p>
      )}
    </div>
  )
}

/** The pill that travels with a planned figure. */
export function BasisPill({ children }: { children: ReactNode }) {
  return (
    <span
      className="type-column-label px-2.5 py-[3px]"
      style={{
        border: '1px solid var(--rule-2)',
        borderRadius: 'var(--radius-pill)',
        background: 'var(--paper)',
        color: 'var(--ink-2)',
        fontWeight: 700,
      }}
    >
      {children}
    </span>
  )
}

/**
 * A ratio, always written as n / total beside its bar. A percentage on its own
 * hides how few records the share was drawn from, and on this programme that is
 * the whole question.
 */
export function RatioMeter({
  label,
  icon,
  count,
  total,
}: {
  label: string
  icon?: ReactNode
  count: number
  total: number
}) {
  const pct = total > 0 ? Math.min((count / total) * 100, 100) : 0

  return (
    <div className="flex flex-col gap-[5px]">
      <div className="grid items-baseline gap-2.5" style={{ gridTemplateColumns: 'minmax(0, 1fr) auto' }}>
        <span
          className="inline-flex items-center gap-[7px]"
          style={{ fontSize: 13, color: 'var(--ink-2)' }}
        >
          {icon}
          {label}
        </span>
        <span className="tabular font-semibold" style={{ fontSize: 15 }}>
          {count} / {total}
        </span>
      </div>
      <div
        className="flex overflow-hidden"
        style={{
          height: 8,
          borderRadius: 'var(--radius-pill)',
          background: 'var(--paper)',
          border: '1px solid var(--rule)',
        }}
      >
        <span style={{ width: `${pct}%`, background: 'var(--green-ink)', display: 'block' }} />
      </div>
    </div>
  )
}

/**
 * A flat block the size of what replaces it. No shimmer and no pulse: an
 * animated skeleton repaints every frame on a phone that has better things to
 * do, and a footprint that matches means the row never reflows twice.
 */
export function TileSkeleton({ rows = 3 }: { rows?: number }) {
  const bar = (width: string, height: number): CSSProperties => ({
    width,
    height,
    borderRadius: 6,
    background: 'var(--sand-2)',
  })

  return (
    <div aria-hidden className="flex flex-col gap-2.5">
      <span style={bar('60%', 13)} />
      <span style={bar('45%', 30)} />
      {Array.from({ length: rows }, (_, index) => (
        <span key={index} style={bar('100%', 15)} />
      ))}
    </div>
  )
}
