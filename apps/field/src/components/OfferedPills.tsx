import { useTranslation } from 'react-i18next'

/** How a machine is offered — to rent, to buy, or both — as two quiet pills. */
export function OfferedPills({
  canRent,
  canBuy,
  testId,
}: {
  canRent: boolean | null | undefined
  canBuy: boolean | null | undefined
  testId?: string
}) {
  const { t } = useTranslation()
  const modes = [canRent && 'rent', canBuy && 'buy'].filter(Boolean) as Array<'rent' | 'buy'>

  return (
    <span data-testid={testId} className="inline-flex flex-wrap gap-1.5">
      {modes.map((mode) => (
        <span
          key={mode}
          className="type-microlabel px-2 py-px"
          style={{
            border: '1px solid var(--primary)',
            borderRadius: 'var(--radius-pill)',
            background: 'var(--primary-tint)',
            color: 'var(--primary-ink)',
          }}
        >
          {t(`resources.offered.${mode}`)}
        </span>
      ))}
    </span>
  )
}

/** What a request asked for: to rent the machine, or to buy it. */
export function AcquisitionPill({ mode, testId = 'request-acquisition' }: { mode: 'rent' | 'buy' | null | undefined; testId?: string }) {
  const { t } = useTranslation()
  if (!mode) return null
  return (
    <span
      data-testid={testId}
      className="type-microlabel px-2 py-px"
      style={{
        border: '1px solid var(--rule-2)',
        borderRadius: 'var(--radius-pill)',
        background: 'var(--paper)',
        color: 'var(--ink-2)',
      }}
    >
      {t(`resources.acquisition.${mode}`)}
    </span>
  )
}
