import { useRef, useState, type FormEvent } from 'react'
import { Notice } from '@genesy/ui/components/notice'
import { useTranslation } from 'react-i18next'

import { ConfirmDialog } from '@/components/ConfirmDialog'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { BangMark } from '@/components/marks'
import { fieldErrorsOf } from '@/lib/apiError'
import { errorText } from '@/lib/errors'
import { FormField } from '@/components/FormField'
import { PageHeader } from '@/components/PageHeader'
import { CONTROL } from '@/components/controlStyles'
import { Card, Loading } from '@/components/controls'
import { Button } from '@/components/ui/button'
import { type SupplyLink, useGrantLink, useRevokeLink, useSupplyLinks } from '@/features/ops/supply/useSupply'
import { formatTimestamp } from '@/lib/format'
import { newUuid } from '@/lib/ids'

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
      {revoke.isSuccess && (
        <div data-testid="supply-revoke-done">
          <Notice variant="success" title={t('supply.revoked')} />
        </div>
      )}
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
                  <p className="font-semibold">{link.partner.display_name}</p>
                  <p className="type-note" style={{ color: 'var(--ink-2)' }}>
                    {t(`supply.status.${link.status}`)} · {t('supply.grantedOn', { date: formatTimestamp(link.granted_at) })}
                  </p>
                  <p className="type-note" style={{ color: 'var(--ink-3)' }}>
                    {link.fields.length
                      ? link.fields.map((f) => t(`supply.field.${f}`, { defaultValue: t('common.other') })).join(', ')
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
          detail={t('supply.revokeDetail', { partner: revoking.partner.display_name })}
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
  /** Kept until a link is granted, edits included, so a lost reply can never turn into a second link. */
  const idempotencyKey = useRef(newUuid())

  // The API's refusal of the shop or the fields is shown at it, until the next edit.
  const refused = fieldErrorsOf(grant.error, (key) =>
    key === 'to_workspace_id' ? key : /^fields(\.\d+)?$/.test(key) ? 'fields' : null)
  const fieldError = (field: string) => (refused.fields[field] ? errorText(refused.fields[field], t) : undefined)
  const edit = () => {
    if (grant.isError || grant.isSuccess) grant.reset()
  }

  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (inFlight.current) return
    inFlight.current = true
    grant.mutate(
      { toWorkspaceId: to.trim(), fields, idempotencyKey: idempotencyKey.current },
      {
        onSuccess: () => {
          idempotencyKey.current = newUuid()
          setTo('')
          setFields(() => [])
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
        <FormField label={t('supply.grantTo')} id="supply-grant-to" hint={t('supply.grantToHint')} error={fieldError('to_workspace_id')} errorTestId="supply-grant-to-error">
          <input
            id="supply-grant-to"
            data-testid="supply-grant-to"
            value={to}
            onChange={(e) => {
              setTo(e.target.value)
              edit()
            }}
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
                  onChange={(e) => {
                    setFields((now) => (e.target.checked ? [...now, field] : now.filter((f) => f !== field)))
                    edit()
                  }}
                />
                {t(`supply.field.${field}`)}
              </label>
            ))}
          </div>
          {fieldError('fields') && (
            <p data-testid="supply-grant-fields-error" className="flex items-start gap-[7px] font-medium" style={{ fontSize: 13, color: 'var(--flag-ink)' }}>
              <BangMark />
              {fieldError('fields')}
            </p>
          )}
        </fieldset>
        {grant.error && refused.general != null ? (
          <div data-testid="supply-grant-error">
            <ErrorState error={refused.general} />
          </div>
        ) : null}
        {grant.isSuccess && (
          <div data-testid="supply-grant-done">
            <Notice variant="success" title={t('supply.granted')} />
          </div>
        )}
        <div>
          <Button type="submit" data-testid="supply-grant-submit" disabled={grant.isPending}>
            {grant.isPending ? t('supply.granting') : t('supply.grantSubmit')}
          </Button>
        </div>
      </form>
    </Card>
  )
}
