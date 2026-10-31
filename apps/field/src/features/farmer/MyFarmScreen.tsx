import { Weight } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { Loading } from '@/components/controls'
import { ProvenanceBadge } from '@/components/ProvenanceBadge'
import { useMyFarm, type MyFarm } from '@/features/farmer/useMyFarm'
import { formatArea, formatKg, formatPlainDate } from '@/lib/format'

/**
 * Spec 6.2 — read-only. Household, plots with area, crops with the current
 * expected harvest, every figure carrying its provenance.
 *
 * Nothing here can be edited: a farmer's records are changed by a field
 * officer, and offering a control the policies would refuse would be a lie.
 */
export function MyFarmScreen() {
  const { t } = useTranslation()
  const query = useMyFarm()

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  if (query.isLoading) {
    return <Loading testId="my-farm-loading" />
  }

  // A real message, not an empty list: zero farms means none has been
  // registered yet, which is a normal state for a new farmer.
  if (query.farms.length === 0) {
    return <EmptyState title={t('myFarm.noFarmTitle')} detail={t('myFarm.noFarmDetail')} />
  }

  return (
    <section className="flex max-w-lg flex-col gap-4" data-testid="my-farm">
      <header className="flex flex-col gap-1.5">
        <h1 className="type-screen-title">{t('myFarm.title')}</h1>
        <p style={{ fontSize: 13, lineHeight: 1.5, color: 'var(--ink-2)', textWrap: 'pretty' }}>
          {t('myFarm.readOnly')}
        </p>
      </header>

      {query.farms.map((farm) => (
        <article
          key={farm.id}
          data-testid="farm-card"
          className="flex flex-col gap-3 p-4"
          style={{
            border: '1px solid var(--rule)',
            borderRadius: 'var(--radius-frame)',
            background: 'var(--paper)',
          }}
        >
          {/* Farm level keeps the full lozenge. */}
          <div className="flex flex-wrap items-center gap-2.5">
            <h2 style={{ fontSize: 17, fontWeight: 600 }}>{farm.label}</h2>
            <Badge record={farm} />
          </div>

          {farm.plots.length === 0 ? (
            <EmptyState title={t('myFarm.noPlots')} />
          ) : (
            farm.plots.map((plot) => <Plot key={plot.id} plot={plot} />)
          )}
        </article>
      ))}
    </section>
  )
}

function Plot({ plot }: { plot: MyFarm['plots'][number] }) {
  const { t } = useTranslation()

  return (
    <div
      data-testid="plot-card"
      className="flex flex-col gap-2.5 pt-3"
      style={{ borderTop: '1px solid var(--rule)' }}
    >
      {/*
        Provenance collapses in the nest. Plot and cycle lines carry the mark
        plus one line of words, so four full lozenges down a card stop competing
        with the kilograms.
      */}
      <div className="flex flex-wrap items-center gap-2.5">
        <span style={{ fontSize: 16, fontWeight: 600 }}>{plot.label}</span>
        <span className="tabular" style={{ fontSize: 14, color: 'var(--ink-2)' }}>
          {formatArea(plot.area_ha, 'hectare')}
        </span>
      </div>
      <Badge record={plot} compact />

      {plot.cycles.length === 0 ? (
        <p className="type-note" style={{ color: 'var(--ink-3)' }}>
          {t('myFarm.noCycles')}
        </p>
      ) : (
        plot.cycles.map((cycle) => (
          <div
            key={cycle.id}
            data-testid="cycle-card"
            className="ml-1 flex flex-col gap-1.5 pl-3.5"
            style={{ borderLeft: '3px solid var(--rule)' }}
          >
            <div className="flex flex-wrap items-baseline gap-2.5">
              <span style={{ fontSize: 16, fontWeight: 600 }}>{cycle.crop_name}</span>
              <span className="type-note" style={{ color: 'var(--ink-3)' }}>
                {formatPlainDate(cycle.harvest_start)} – {formatPlainDate(cycle.harvest_end)}
              </span>
            </div>

            {cycle.expected && (
              <div
                className="grid items-baseline gap-3"
                style={{ gridTemplateColumns: 'minmax(0, 1fr) auto' }}
              >
                <span
                  className="inline-flex items-center gap-[7px]"
                  style={{ fontSize: 13, color: 'var(--ink-2)' }}
                >
                  <Weight aria-hidden size={15} strokeWidth={2.25} style={{ flex: 'none' }} />
                  {t('myFarm.expected')}
                </span>
                <span className="tabular font-semibold" style={{ fontSize: 17 }}>
                  {formatKg(cycle.expected.quantity_kg)}
                </span>
              </div>
            )}
            <Badge record={cycle.expected ?? cycle} compact />
          </div>
        ))
      )}
    </div>
  )
}

function Badge({
  record,
  compact = false,
}: {
  compact?: boolean
  record: {
    source: Parameters<typeof ProvenanceBadge>[0]['source']
    verification: Parameters<typeof ProvenanceBadge>[0]['verification']
    confidence: Parameters<typeof ProvenanceBadge>[0]['confidence']
    captured_at: string
  }
}) {
  return (
    <ProvenanceBadge
      source={record.source}
      verification={record.verification}
      confidence={record.confidence}
      capturedAt={record.captured_at}
      compact={compact}
    />
  )
}
