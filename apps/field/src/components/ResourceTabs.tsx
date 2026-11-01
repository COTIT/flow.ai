import { useTranslation } from 'react-i18next'

import type { ResourceKind } from '@/features/farmer/resourceKind'

const KINDS: ResourceKind[] = ['equipment', 'loan']

/** Equipment | Loans — one choice, two lists, the choice held in the URL. */
export function ResourceTabs({
  value,
  onChange,
  testIdPrefix,
}: {
  value: ResourceKind
  onChange: (kind: ResourceKind) => void
  testIdPrefix: string
}) {
  const { t } = useTranslation()

  return (
    <div role="tablist" aria-label={t('resources.kindLabel')} className="inline-flex w-fit gap-1 p-1"
      style={{ border: '1px solid var(--rule)', borderRadius: 'var(--radius-pill)', background: 'var(--paper)' }}
    >
      {KINDS.map((kind) => {
        const selected = kind === value
        return (
          <button
            key={kind}
            type="button"
            role="tab"
            aria-selected={selected}
            data-testid={`${testIdPrefix}-tab-${kind}`}
            onClick={() => onChange(kind)}
            className="px-4 font-semibold"
            style={{
              minHeight: 40,
              borderRadius: 'var(--radius-pill)',
              border: 0,
              background: selected ? 'var(--primary)' : 'transparent',
              color: selected ? '#fff' : 'var(--ink-2)',
              fontSize: 14,
              fontFamily: 'inherit',
            }}
          >
            {t(`resources.kind.${kind}`)}
          </button>
        )
      })}
    </div>
  )
}
