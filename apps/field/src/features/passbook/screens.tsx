import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { Card, Loading } from '@/components/controls'
import { PassbookView } from '@/features/passbook/PassbookView'
import { PayOut } from '@/features/passbook/PayOut'
import { useMyPassbook, usePayouts, usePersonPassbook } from '@/features/passbook/usePassbook'
import { apiStatus } from '@/lib/apiError'
import { formatPrice } from '@/lib/format'

type LinkTo = Parameters<typeof Link>[0]['to']

/** `GET /passbook/mine` is 404 for an account that is no registered farmer: an answer, not a failure. */
const notRegistered = (error: unknown) => apiStatus(error) === 404

/** `/farm/passbook`: the farmer's own lines and payouts, read-only. */
export function MyPassbookScreen() {
  const { t } = useTranslation()
  const passbook = useMyPassbook()
  const payouts = usePayouts()
  return (
    <section data-testid="my-passbook" className="flex flex-col gap-4">
      <h1 className="type-screen-title">{t('passbook.mine')}</h1>
      {notRegistered(passbook.error) ? (
        <div data-testid="passbook-not-registered">
          <EmptyState title={t('passbook.notRegisteredTitle')} detail={t('passbook.notRegisteredDetail')} />
        </div>
      ) : (
        <PassbookView passbook={passbook} payouts={payouts} />
      )}
    </section>
  )
}

/** The farmer home's passbook card: balance per currency and the way in. Hidden for a non-farmer. */
export function MyPassbookCard() {
  const { t, i18n } = useTranslation()
  const passbook = useMyPassbook()
  if (notRegistered(passbook.error)) return null
  return (
    <Card className="flex flex-col gap-2.5 p-4">
      <section data-testid="farm-home-passbook" className="flex flex-col gap-2.5">
        <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
          {t('passbook.mine')}
        </h2>
        {passbook.error ? (
          <ErrorState error={passbook.error} onRetry={() => void passbook.refetch()} />
        ) : !passbook.data ? (
          <Loading testId="passbook-home-loading" />
        ) : passbook.data.balances.length === 0 ? (
          <p data-testid="passbook-home-empty" className="type-note">
            {t('passbook.emptyTitle')}
          </p>
        ) : (
          passbook.data.balances.map((b) => (
            <p key={b.currency_id} data-testid={`passbook-home-${b.currency}`} className="tabular" style={{ fontSize: 22, fontWeight: 600 }}>
              {formatPrice(b, i18n.resolvedLanguage ?? 'en')}
            </p>
          ))
        )}
        <Link to={'/farm/passbook' as LinkTo} className="font-medium underline">
          {t('passbook.open')}
        </Link>
      </section>
    </Card>
  )
}

/** Person page (staff only): the person's passbook and payouts, and Pay out per positive balance. */
export function PersonPassbookSection({ personId, personName }: { personId: string; personName: string }) {
  const passbook = usePersonPassbook(personId)
  const payouts = usePayouts(personId)
  return (
    <PassbookView
      passbook={passbook}
      payouts={payouts}
      action={(balance) => <PayOut personId={personId} personName={personName} balance={balance} />}
    />
  )
}
