import { getRouteApi, Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useScopeNames } from '@/app/scope'
import { DrillLink } from '@/components/DrillLink'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { PageHeader } from '@/components/PageHeader'
import { TableCard } from '@/components/controls'
import { TileSkeleton } from '@/features/tower/TowerTile'
import { QUALITY_METRICS, validateQualitySearch } from '@/features/tower/towerSearch'
import { useTowerQuality, useTowerQualityRows } from '@/features/tower/useTower'

const route = getRouteApi('/_ops/ops/tower/quality')

export function TowerQualityScreen() {
  const { t } = useTranslation()
  const { village, metric } = validateQualitySearch(route.useSearch())
  const scope = useScopeNames()
  const totals = useTowerQuality(village)
  const rows = useTowerQualityRows(village, metric)

  const error = totals.error ?? rows.error ?? scope.error
  if (error) return <ErrorState error={error} onRetry={() => void rows.refetch()} />

  const ratio = metric === 'persons'
    ? [totals.data?.persons_verified, totals.data?.persons]
    : metric === 'farms'
      ? [totals.data?.farms_with_gps, totals.data?.farms]
      : [totals.data?.cycles_with_estimate, totals.data?.cycles]
  const labels = {
    persons: t('tower.personsVerified'),
    farms: t('tower.farmsWithGps'),
    cycles: t('tower.cyclesWithEstimate'),
  }

  return (
    <section className="flex w-full flex-col gap-4" data-testid="quality-drill">
      <PageHeader
        backTo="/ops/tower"
        backSearch={{ village }}
        backLabel={t('tower.backToTower')}
        eyebrow={[village ? scope.data?.villages?.[village] : '', t('tower.quality')]
          .filter(Boolean).join(' · ')}
        title={t('tower.qualityDrill')}
        breadcrumbs={[{ label: t('nav.tower'), to: '/ops/tower' }, { label: t('tower.quality') }]}
      />

      <nav aria-label={t('tower.qualityMetrics')} className="flex flex-wrap gap-2">
        {QUALITY_METRICS.map((item) => (
          <Link
            key={item}
            to="/ops/tower/quality"
            search={{ village, metric: item }}
            aria-current={metric === item ? 'page' : undefined}
            className="rounded-md border border-rule px-3 py-2 text-sm font-semibold text-primary-ink"
          >
            {labels[item]}
          </Link>
        ))}
      </nav>

      {totals.isLoading || rows.isLoading ? (
        <TileSkeleton rows={4} />
      ) : !village || !totals.data ? (
        <EmptyState title={t('tower.noDataTitle')} detail={t('tower.noDataDetail')} />
      ) : (
        <>
          <p className="tabular text-sm text-ink-2" data-testid="quality-ratio">
            {labels[metric]}: {ratio[0] ?? 0} / {ratio[1] ?? 0}
          </p>
          {(rows.data ?? []).length === 0 ? (
            <EmptyState title={t('tower.noDataTitle')} detail={t('tower.noDataDetail')} />
          ) : (
            <TableCard>
              <div className="overflow-x-auto">
                <table className="w-full border-collapse text-sm" data-testid="quality-table">
                  <thead>
                    <tr className="bg-sand-2 text-left">
                      <th scope="col" className="px-3.5 py-2.5">{t('tower.qualityRecord')}</th>
                      <th scope="col" className="px-3.5 py-2.5">{t('tower.qualityState')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {(rows.data ?? []).map((row) => (
                      <tr key={row.id} data-testid="quality-row" className="border-t border-rule">
                        <td className="px-3.5 py-3">
                          <DrillLink kind={row.kind} id={row.id}>{row.label}</DrillLink>
                          {row.detail && (
                            <span className="ml-2 text-ink-3">
                              {row.kind === 'person' ? t(`verification.${row.detail}`) : row.detail}
                            </span>
                          )}
                        </td>
                        <td className="px-3.5 py-3">
                          {row.qualifies ? t('tower.qualityCounted') : t('tower.qualityMissing')}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </TableCard>
          )}
        </>
      )}
    </section>
  )
}
