import type { CSSProperties, ReactNode } from 'react'
import { CircleSlash, Lock, Weight } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { formatKg, formatPercent } from '@/lib/format'

export interface CoverageBarProps {
  demandKg: number | null
  availableKg: number | null
  /**
   * Omit entirely where the figure is not available at this level — the Tower's
   * market tile reports on a village and has no committed total to show, and a
   * row reading "—" there would look like a fault rather than a scope.
   */
  committedKg?: number | null
  coveragePct: number | null
  /**
   * What this bar is about — a village name on a demand detail. Defaults to
   * naming the demand the coverage is measured against, which is the only
   * honest reading of a percentage on its own.
   */
  label?: string
}

/**
 * Demand against available supply, with the already-committed slice visible —
 * spec §9.1.
 *
 * The committed slice is the whole point: supply that looks available in a
 * total is not available if it is already promised to a live opportunity. So it
 * is drawn **outside the track**, below a rule, as an outlined swatch. A
 * stacked bar would put it inside the same length as the available supply and
 * imply the two add up to something, which is exactly the claim that is false.
 *
 * The three quantities are told apart three ways — solid fill, a lighter tone,
 * outlined swatch — because colour alone does not survive a scratched screen in
 * sunlight, and there is no chart library here to do it for us.
 *
 * Coverage is NOT computed here. v_demand_match calculates it, and this
 * renders what it was handed — so a disagreement between the percentage and
 * the figures is a view bug to fix in SQL, not something to paper over in the
 * client. The uncovered figure is the one arithmetic this component does, and
 * it is subtraction of two figures already on screen, not an aggregate.
 */
export function CoverageBar({
  demandKg,
  availableKg,
  committedKg,
  coveragePct,
  label,
}: CoverageBarProps) {
  const { t } = useTranslation()

  const uncoveredKg =
    demandKg === null || availableKg === null ? null : Math.max(demandKg - availableKg, 0)

  // Capped so an over-supplied demand does not overflow the track; the figure
  // itself is still reported unmodified above.
  const covered = coveragePct === null ? 0 : Math.min(Math.max(coveragePct, 0), 100)

  return (
    <div className="flex flex-col gap-2.5">
      <div className="flex flex-wrap items-baseline justify-between gap-2.5">
        <span style={{ fontSize: 15, lineHeight: '22px', fontWeight: 600 }}>
          {label ?? t('coverage.coverageOf', { total: formatKg(demandKg) })}
        </span>
        <span data-testid="coverage-pct" className="tabular type-figure">
          {formatPercent(coveragePct)}
        </span>
      </div>

      {coveragePct !== null && (
        <div
          role="meter"
          aria-label={t('coverage.label')}
          aria-valuenow={coveragePct}
          aria-valuemin={0}
          aria-valuemax={100}
          className="flex w-full overflow-hidden"
          style={{
            height: 16,
            border: '1px solid var(--rule)',
            borderRadius: 'var(--radius-pill)',
            background: 'var(--sand-2)',
          }}
        >
          <span
            data-segment="available"
            style={{ width: `${covered}%`, background: 'var(--accent)', display: 'block' }}
          />
          <span
            data-segment="uncovered"
            style={{
              width: `${100 - covered}%`,
              background: 'var(--sand-2)',
              display: 'block',
            }}
          />
        </div>
      )}

      <div data-testid="coverage-legend" className="flex flex-col gap-[5px]" style={{ fontSize: 13 }}>
        <LegendRow
          swatch="available"
          icon={<Weight aria-hidden size={15} strokeWidth={2.25} style={{ flex: 'none' }} />}
          label={t('coverage.availableNow')}
          testId="coverage-available"
          value={formatKg(availableKg)}
        />
        <LegendRow
          swatch="uncovered"
          icon={<CircleSlash aria-hidden size={15} strokeWidth={2.25} style={{ flex: 'none' }} />}
          // The demand total lives inside this label: what is not covered is
          // only meaningful against the figure it was measured against.
          label={t('coverage.notCoveredOf', { total: formatKg(demandKg) })}
          labelTestId="coverage-demand"
          testId="coverage-uncovered"
          value={formatKg(uncoveredKg)}
        />
        {committedKg !== undefined && (
          <LegendRow
            swatch="committed"
            icon={<Lock aria-hidden size={15} strokeWidth={2.25} style={{ flex: 'none' }} />}
            label={t('coverage.committed')}
            testId="coverage-committed"
            value={formatKg(committedKg)}
            // Below the rule: it is not part of what the bar measures.
            ruled
          />
        )}
      </div>

      <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
        {t('coverage.committedNote')}
      </p>
    </div>
  )
}

/**
 * A label that can wrap to two lines while the figure stays pinned to the right
 * edge. A flex row with `justify-between` cannot do that, and Kiswahili runs
 * 10–30% longer than the English this was laid out in.
 */
function LegendRow({
  swatch,
  icon,
  label,
  labelTestId,
  testId,
  value,
  ruled = false,
}: {
  swatch: keyof typeof SWATCH
  icon: ReactNode
  label: string
  labelTestId?: string
  testId: string
  value: string
  ruled?: boolean
}) {
  return (
    <div
      className="grid items-baseline gap-3"
      style={{
        gridTemplateColumns: 'minmax(0, 1fr) auto',
        ...(ruled ? { borderTop: '1px solid var(--rule)', paddingTop: 5 } : {}),
      }}
    >
      <span
        data-testid={labelTestId}
        className="inline-flex items-center gap-[7px]"
        style={{ color: 'var(--ink-2)' }}
      >
        <span data-swatch={swatch} aria-hidden style={SWATCH[swatch]} />
        {icon}
        {label}
      </span>
      <span data-testid={testId} className="tabular font-semibold">
        {value}
      </span>
    </div>
  )
}

/** Solid accent means available. Pale means missing. Outlined means spoken for. */
const SWATCH: Record<'available' | 'uncovered' | 'committed', CSSProperties> = {
  available: { width: 10, height: 10, borderRadius: 3, background: 'var(--accent)', flex: 'none' },
  uncovered: {
    width: 10,
    height: 10,
    borderRadius: 3,
    background: 'var(--sand-2)',
    border: '1px solid var(--rule-2)',
    flex: 'none',
  },
  committed: {
    width: 10,
    height: 10,
    borderRadius: 3,
    border: '2px solid var(--ink-2)',
    boxSizing: 'border-box',
    flex: 'none',
  },
}
