import type { ReactNode } from 'react'
import { BatteryCharging, Zap } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { computeEstimate } from '@/features/farmer/energyEstimate'
import { formatKw, formatKwh } from '@/lib/format'

export interface EnergyEstimatePanelProps {
  ratedPowerKw: number
  quantity: number
  hoursPerDay: number
  daysPerWeek: number
}

/**
 * Shows the inputs, the arithmetic and the outputs — spec §9.1.
 *
 * The arithmetic is shown AS arithmetic: three equation lines, each starting
 * from what the line above produced. A two-column list said what the numbers
 * were but not that the second follows from the first, and an estimate that
 * shows its working is harder to mistake for a measurement.
 *
 * Mirrors energy_estimate's generated columns exactly. If the two ever
 * disagree, this component is wrong. It is a PREVIEW only: the stored figure
 * is written by pue_recompute_estimate, which is the sole writer of
 * energy_estimate — clients hold no write policy on it at all.
 *
 * Always labelled an estimate. It is not measured consumption and not a
 * commitment. The dashed border is gone; the header band on a solid
 * ground says "estimate" in words instead.
 */
export function EnergyEstimatePanel({
  ratedPowerKw,
  quantity,
  hoursPerDay,
  daysPerWeek,
}: EnergyEstimatePanelProps) {
  const { t } = useTranslation()
  const estimate = computeEstimate({ ratedPowerKw, quantity, hoursPerDay, daysPerWeek })

  const shown = (value: number) => (Number.isFinite(value) ? value : 0)

  return (
    <section
      data-testid="estimate-panel"
      className="flex flex-col overflow-hidden"
      style={{
        border: '1px solid var(--rule-2)',
        borderRadius: 'var(--radius-frame)',
        background: 'var(--paper)',
      }}
    >
      <header
        className="flex flex-col gap-[5px] px-4 py-3.5"
        style={{
          background: 'var(--sand-2)',
          borderBottom: '1px solid var(--rule-2)',
        }}
      >
        <h3
          className="flex flex-wrap items-center gap-2"
          style={{ fontSize: 15, lineHeight: '22px', fontWeight: 600 }}
        >
          {t('estimate.title')}
          <span
            className="px-2 py-0.5"
            style={{
              border: '1px solid var(--rule-2)',
              borderRadius: 'var(--radius-pill)',
              background: 'var(--paper)',
              fontSize: 12,
              lineHeight: '16px',
              fontWeight: 700,
              letterSpacing: '.08em',
              textTransform: 'uppercase',
              color: 'var(--ink-2)',
            }}
          >
            {t('estimate.tag')}
          </span>
        </h3>
        <p style={{ fontSize: 13, lineHeight: '19px', fontWeight: 500 }}>{t('estimate.isEstimate')}</p>
        <p data-testid="estimate-basis" className="type-note" style={{ color: 'var(--ink-2)' }}>
          {t('estimate.basis')}
        </p>
      </header>

      <div className="flex flex-col gap-3 p-4">
        <div className="flex flex-col gap-[9px]">
          <Equation
            name="peak"
            terms={[
              <Term key="rated" testId="estimate-rated-power" value={formatKw(ratedPowerKw)} unit={t('estimate.rated')} />,
              <Operator key="x1">×</Operator>,
              <Term key="qty" testId="estimate-quantity" value={String(shown(quantity))} unit={t('estimate.units', { count: shown(quantity) })} />,
              <Operator key="e1">=</Operator>,
            ]}
            result={
              <Result testId="estimate-power" icon={<Zap aria-hidden size={16} strokeWidth={2.25} style={{ flex: 'none' }} />}>
                {formatKw(estimate.estPowerKw)}
              </Result>
            }
            resultLabel={t('estimate.peak')}
          />

          <Equation
            name="day"
            ruled
            terms={[
              <Carried key="peak" value={formatKw(estimate.estPowerKw)} />,
              <Operator key="x2">×</Operator>,
              <Term key="hours" testId="estimate-hours" value={String(shown(hoursPerDay))} unit={t('estimate.hoursADay')} />,
              <Operator key="e2">=</Operator>,
            ]}
            result={
              <Result testId="estimate-kwh-day" icon={<BatteryCharging aria-hidden size={16} strokeWidth={2.25} style={{ flex: 'none' }} />}>
                {formatKwh(estimate.estKwhPerDay)}
              </Result>
            }
            resultLabel={t('estimate.aDay')}
          />

          <Equation
            name="week"
            ruled
            terms={[
              <Carried key="day" value={formatKwh(estimate.estKwhPerDay)} />,
              <Operator key="x3">×</Operator>,
              <Term key="days" testId="estimate-days" value={String(shown(daysPerWeek))} unit={t('estimate.daysAWeek')} />,
              <Operator key="e3">=</Operator>,
            ]}
            result={
              <Result testId="estimate-kwh-week" icon={<BatteryCharging aria-hidden size={16} strokeWidth={2.25} style={{ flex: 'none' }} />}>
                {formatKwh(estimate.estKwhPerWeek)}
              </Result>
            }
            resultLabel={t('estimate.aWeek')}
          />
        </div>

        <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
          {t('estimate.peakNote')}
        </p>
      </div>
    </section>
  )
}

/** One line of the working. Wraps on a narrow phone rather than scrolling. */
function Equation({
  name,
  terms,
  result,
  resultLabel,
  ruled = false,
}: {
  name: string
  terms: ReactNode[]
  result: ReactNode
  resultLabel: string
  ruled?: boolean
}) {
  return (
    <div
      data-equation={name}
      className="flex flex-wrap items-baseline gap-2"
      style={{
        fontSize: 13,
        lineHeight: '19px',
        color: 'var(--ink-2)',
        ...(ruled ? { borderTop: '1px solid var(--rule)', paddingTop: 9 } : {}),
      }}
    >
      {terms}
      {result}
      {resultLabel}
    </div>
  )
}

/** An input the user controls, with the word that says what it is. */
function Term({ testId, value, unit }: { testId: string; value: string; unit: string }) {
  return (
    <>
      <span data-testid={testId} className="tabular font-semibold" style={{ color: 'var(--ink)' }}>
        {value}
      </span>
      {unit}
    </>
  )
}

/** The figure the previous line produced, restated so the chain is visible. */
function Carried({ value }: { value: string }) {
  return (
    <span className="tabular font-semibold" style={{ color: 'var(--ink)' }}>
      {value}
    </span>
  )
}

function Operator({ children }: { children: ReactNode }) {
  return <span style={{ color: 'var(--ink-3)' }}>{children}</span>
}

/** What the line adds up to, at 17px with its unit mark. */
function Result({
  testId,
  icon,
  children,
}: {
  testId: string
  icon: ReactNode
  children: ReactNode
}) {
  return (
    <span
      data-testid={testId}
      className="tabular inline-flex items-center gap-1.5 font-semibold"
      style={{ fontSize: 17, lineHeight: '24px', color: 'var(--ink)' }}
    >
      {icon}
      {children}
    </span>
  )
}
