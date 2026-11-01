import { useRef, useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { ConfirmDialog } from '@/components/ConfirmDialog'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { FormField } from '@/components/FormField'
import { PageHeader } from '@/components/PageHeader'
import { CONTROL } from '@/components/controlStyles'
import { Card, Loading } from '@/components/controls'
import { Button } from '@/components/ui/button'
import { type SupplyLink, useGrantLink, useRevokeLink, useSupplyLinks } from '@/features/ops/supply/useSupply'
import { formatTimestamp } from '@/lib/format'

/**
 * The fields `supply-links.md` accepts today. `farmer_names` and `farmer_photos` are refused until the
 * data-protection gate opens, so they are not offered.
 */
const FIELDS = ['village', 'farmer_count', 'harvest_window'] as const

/** `/ops/supply-links`: links this workspace grants to a shop, granted and revoked here. */
export function SupplyLinksScreen() {
  const { t } = useTranslation()
  const links = useSupplyLinks()
  const revoke = useRevokeLink()
  const [revoking, setRevoking] = useState<SupplyLink | null>(null)
  const revokeInFlight = useRef(false)

  const confirmRevoke = () => {
    if (!revoking || revokeInFlight.current) return
    revokeInFlight.current = true
    revoke.mutate(revoking.id, {
      onSettled: () => {
        revokeInFlight.current = false
        setRevoking(null)
      },
    })
  }

  return (
    <section data-testid="supply-links-screen" className="flex flex-col gap-4">
      <PageHeader title={t('supply.linksTitle')} description={t('supply.linksIntro')} />
      <GrantForm />
      {revoke.error ? <ErrorState error={revoke.error} /> : null}
      {links.error ? (
        <ErrorState error={links.error} onRetry={() => void links.refetch()} />
      ) : links.isLoading ? (
        <Loading testId="supply-links-loading" />
      ) : !links.data?.length ? (
        <EmptyState title={t('supply.noneTitle')} detail={t('supply.noneDetail')} />
      ) : (
        <ul className="flex flex-col gap-2">
          {links.data.map((link) => (
            <li key={link.id} data-testid="supply-link-row">
              <Card className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex min-w-0 flex-col gap-1">
                  <p className="font-semibold">{link.partner.name}</p>
                  <p className="type-note" style={{ color: 'var(--ink-2)' }}>
                    {t(`supply.status.${link.status}`)} · {t('supply.grantedOn', { date: formatTimestamp(link.granted_at) })}
                  </p>
                  <p className="type-note" style={{ color: 'var(--ink-3)' }}>
                    {link.fields.length
                      ? link.fields.map((f) => t(`supply.field.${f}`, { defaultValue: f })).join(', ')
                      : t('supply.noFields')}
                  </p>
                </div>
                {link.status !== 'revoked' && (
                  <Button type="button" variant="secondary" data-testid="supply-link-revoke" onClick={() => setRevoking(link)}>
                    {t('supply.revoke')}
                  </Button>
                )}
              </Card>
            </li>
          ))}
        </ul>
      )}
      {revoking && (
        <ConfirmDialog
          title={t('supply.revokeTitle')}
          detail={t('supply.revokeDetail', { partner: revoking.partner.name })}
          confirmLabel={t('supply.revoke')}
          cancelLabel={t('supply.cancel')}
          confirming={revoke.isPending}
          onConfirm={confirmRevoke}
          onCancel={() => setRevoking(null)}
        />
      )}
    </section>
  )
}

function GrantForm() {
  const { t } = useTranslation()
  const grant = useGrantLink()
  const [to, setTo] = useState('')
  const [fields, setFields] = useState<string[]>([])
  const inFlight = useRef(false)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (inFlight.current) return
    inFlight.current = true
    grant.mutate(
      { toWorkspaceId: to.trim(), fields },
      {
        onSuccess: () => {
          setTo('')
          setFields([])
        },
        onSettled: () => (inFlight.current = false),
      },
    )
  }

  return (
    <Card>
      <form data-testid="supply-grant" className="flex flex-col gap-3" onSubmit={submit}>
        <h2 className="font-semibold">{t('supply.grantTitle')}</h2>
        {/* shortcut: the shop is named by its workspace id, the API has no directory of workspaces to pick from; replace with a picker when one is documented. */}
        <FormField label={t('supply.grantTo')} id="supply-grant-to" hint={t('supply.grantToHint')}>
          <input
            id="supply-grant-to"
            data-testid="supply-grant-to"
            value={to}
            onChange={(e) => setTo(e.target.value)}
            className="w-full"
            style={CONTROL}
            autoComplete="off"
          />
        </FormField>
        <fieldset className="flex flex-col gap-1.5">
          <legend style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}>{t('supply.grantFields')}</legend>
          <div className="flex flex-wrap gap-x-4 gap-y-2">
            {FIELDS.map((field) => (
              <label key={field} className="inline-flex items-center gap-2" style={{ minHeight: 40 }}>
                <input
                  type="checkbox"
                  data-testid={`supply-grant-field-${field}`}
                  checked={fields.includes(field)}
                  onChange={(e) =>
                    setFields((now) => (e.target.checked ? [...now, field] : now.filter((f) => f !== field)))
                  }
                />
                {t(`supply.field.${field}`)}
              </label>
            ))}
          </div>
        </fieldset>
        {grant.error ? (
          <div data-testid="supply-grant-error">
            <ErrorState error={grant.error} />
          </div>
        ) : null}
        <div>
          <Button type="submit" data-testid="supply-grant-submit" disabled={grant.isPending}>
            {grant.isPending ? t('supply.granting') : t('supply.grantSubmit')}
          </Button>
        </div>
      </form>
    </Card>
  )
}
