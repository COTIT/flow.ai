import { useCallback, useEffect, useId, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { ConfirmDialog } from '@/components/ConfirmDialog'
import { ErrorState } from '@/components/ErrorState'
import { Button } from '@/components/ui/button'
import { useFarmerLoginIssue } from '@/features/officer/useFarmerLogin'

/**
 * A farmer's app login: create it, or reset its password, and show the
 * temporary password ONCE.
 *
 * The password exists only in this component's mutation result. It is not a
 * query (nothing refetches or caches it), not a draft, not in a URL and never
 * logged; Done clears it, and unmounting drops it (`gcTime: 0`). The next
 * officer to open this person sees who issued a login and when — never what
 * the password was.
 *
 * `autoIssue` is for registration: the login is created as the card mounts,
 * once. `hasLogin` is for a person who already has one: the action becomes a
 * reset, behind a confirmation, because it signs the farmer out.
 *
 * Whether a person may have a login — a phone on record, one of your villages,
 * not a staff account — is the database's decision, and its refusal is the
 * message shown.
 */
export function FarmerLoginCard({
  personId,
  autoIssue = false,
  hasLogin = false,
}: {
  personId: string
  autoIssue?: boolean
  hasLogin?: boolean
}) {
  const { t } = useTranslation()
  const issue = useFarmerLoginIssue(personId)
  const [issuedHere, setIssuedHere] = useState(false)
  const [confirmingReset, setConfirmingReset] = useState(false)
  const started = useRef(false)

  // Once a login exists, "create" is no longer honest: the same call would
  // reset the password just handed over.
  const loginExists = hasLogin || issuedHere

  // `mutate` is stable for the life of the mutation observer.
  const { mutate } = issue
  const run = useCallback(
    () => mutate(undefined, { onSuccess: () => setIssuedHere(true) }),
    [mutate],
  )

  useEffect(() => {
    // A ref, not state: StrictMode runs this effect twice, and a second call
    // would reset the login the first one created.
    if (!autoIssue || started.current) return
    started.current = true
    run()
  }, [autoIssue, run])

  const state = issue.isSuccess ? 'issued' : issue.isError ? 'error' : issue.isPending ? 'pending' : 'idle'

  return (
    <div data-testid="farmer-login-card" data-state={state} className="flex flex-col gap-3">
      {issue.isSuccess ? (
        <Credential
          phone={issue.data.phone}
          password={issue.data.temp_password}
          onDone={() => issue.reset()}
        />
      ) : issue.isError ? (
        <div data-testid="farmer-login-error" className="flex flex-col gap-2.5">
          {!loginExists && <p className="type-body-strong">{t('farmerLogin.failed')}</p>}
          <ErrorState error={issue.error} />
          <Button
            variant="secondary"
            size="lg"
            data-testid="farmer-login-retry"
            className="w-full sm:w-fit"
            onClick={run}
          >
            {t('farmerLogin.retry')}
          </Button>
        </div>
      ) : autoIssue && !loginExists ? (
        <p role="status" className="type-body" style={{ color: 'var(--ink-2)' }}>
          {t('farmerLogin.issuing')}
        </p>
      ) : loginExists ? (
        <Button
          variant="secondary"
          size="lg"
          data-testid="farmer-login-reset"
          className="w-full sm:w-fit"
          disabled={issue.isPending}
          onClick={() => setConfirmingReset(true)}
        >
          {issue.isPending ? t('farmerLogin.resetting') : t('farmerLogin.reset')}
        </Button>
      ) : (
        <Button
          size="lg"
          data-testid="farmer-login-issue"
          className="w-full sm:w-fit"
          disabled={issue.isPending}
          onClick={run}
        >
          {issue.isPending ? t('farmerLogin.issuing') : t('farmerLogin.issue')}
        </Button>
      )}

      {confirmingReset && (
        <ConfirmDialog
          title={t('farmerLogin.resetConfirmTitle')}
          detail={t('farmerLogin.resetConfirmDetail')}
          confirmLabel={t('farmerLogin.resetConfirm')}
          cancelLabel={t('farmerLogin.cancel')}
          onCancel={() => setConfirmingReset(false)}
          onConfirm={() => {
            setConfirmingReset(false)
            run()
          }}
        />
      )}
    </div>
  )
}

function Credential({
  phone,
  password,
  onDone,
}: {
  phone: string
  password: string
  onDone: () => void
}) {
  const { t } = useTranslation()
  const titleId = useId()

  return (
    <section
      aria-labelledby={titleId}
      className="flex flex-col gap-3 p-4"
      style={{
        border: '1.5px solid var(--primary)',
        borderRadius: 'var(--radius-card)',
        background: 'var(--paper)',
      }}
    >
      <h3 id={titleId} className="type-body-strong">
        {t('farmerLogin.title')}
      </h3>
      <p className="type-small" style={{ color: 'var(--ink-2)', textWrap: 'pretty' }}>
        {t('farmerLogin.detail')}
      </p>

      <dl className="flex flex-col gap-3 sm:flex-row sm:gap-8">
        <div className="flex flex-col gap-0.5">
          <dt className="type-note" style={{ color: 'var(--ink-2)' }}>
            {t('farmerLogin.phone')}
          </dt>
          <dd data-testid="farmer-login-phone" className="type-figure tabular select-all" translate="no">
            {phone}
          </dd>
        </div>
        <div className="flex flex-col gap-0.5">
          <dt className="type-note" style={{ color: 'var(--ink-2)' }}>
            {t('farmerLogin.password')}
          </dt>
          {/* Letter-spaced so it can be read aloud and copied by hand. */}
          <dd
            data-testid="farmer-login-password"
            className="type-figure tabular select-all"
            translate="no"
            style={{ letterSpacing: '0.12em' }}
          >
            {password}
          </dd>
        </div>
      </dl>

      <p
        role="note"
        className="type-body-strong px-3.5 py-2.5"
        style={{
          background: 'var(--flag-tint)',
          color: 'var(--flag-ink)',
          borderLeft: '4px solid var(--flag-ink)',
          borderRadius: 'var(--radius-control)',
          textWrap: 'pretty',
        }}
      >
        {t('farmerLogin.shownOnce')}
      </p>

      <Button variant="secondary" size="lg" data-testid="farmer-login-done" className="w-full sm:w-fit" onClick={onDone}>
        {t('farmerLogin.done')}
      </Button>
    </section>
  )
}
