import { useState, type FormEvent } from 'react'
import { Camera, CameraOff } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { AuditTimeline } from '@/components/AuditTimeline'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { StatusPill } from '@/components/StatusPill'
import { Loading } from '@/components/controls'
import { CONTROL_FIELD } from '@/components/controlStyles'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/select'
import { QrScanner } from '@/features/officer/QrScanner'
import {
  useVoucherLookup,
  useVoucherRedeem,
  useVoucherTimeline,
  type IdDocumentType,
  type VoucherPreview,
} from '@/features/officer/useVoucherRedeem'
import { Constants } from '@/lib/db.types'
import { dbReasonText } from '@/lib/dbMessages'
import { formatMoney, formatTimestamp } from '@/lib/format'
import { localisedField } from '@/lib/names'
import { formatVoucherCode, normalizeVoucherCode } from '@/lib/voucherCode'

/** The four documents the enum allows — not a list this screen invents. */
const ID_TYPES = Constants.public.Enums.id_document_type

/**
 * `/officer/redeem` — hand over a survey incentive at the office.
 *
 * entry → (scan or type) → lookup → preview → confirm → done
 *
 * An incentive is a fixed cash amount per household per survey, handed over
 * in cash here. Every rule that decides whether THIS staff member may hand it
 * over — four eyes on the household, the audit hold, expiry, a second redeem,
 * the ID and the name check — lives in Postgres. The screen shows the
 * database's answer and its sentence; it never decides.
 *
 * The lookup is a mutation, never a query: each one writes a 'scanned' event
 * to the voucher's audit trail, and a query would refetch on focus and write
 * scans nobody made.
 *
 * The code lives in component state only: never in the URL, a draft or
 * browser storage.
 */
export function RedeemScreen() {
  const { t } = useTranslation()
  const lookup = useVoucherLookup()
  const redeem = useVoucherRedeem()

  const [typed, setTyped] = useState('')
  /** The normalised code the current preview belongs to. */
  const [code, setCode] = useState<string | null>(null)
  const [notAVoucher, setNotAVoucher] = useState(false)
  const [scanning, setScanning] = useState(false)
  const [cameraUnavailable, setCameraUnavailable] = useState(false)
  const [idType, setIdType] = useState<IdDocumentType | ''>('')
  const [nameConfirmed, setNameConfirmed] = useState(false)

  const submit = (raw: string) => {
    const normalized = normalizeVoucherCode(raw)
    redeem.reset()
    setIdType('')
    setNameConfirmed(false)

    // Checked on the device only to spare the audit trail a lookup of
    // something that is not a code at all. The database resolves the code
    // again and is what the answer rests on.
    if (!normalized) {
      lookup.reset()
      setCode(null)
      setNotAVoucher(true)
      return
    }

    setNotAVoucher(false)
    setCode(normalized)
    lookup.mutate(normalized)
  }

  const startOver = () => {
    lookup.reset()
    redeem.reset()
    setTyped('')
    setCode(null)
    setNotAVoucher(false)
    setCameraUnavailable(false)
    setIdType('')
    setNameConfirmed(false)
  }

  const confirm = () => {
    /* c8 ignore next -- the form is only rendered with a looked-up code. */
    if (!code) return
    redeem.mutate({ code, idType: idType || null, nameConfirmed })
  }

  return (
    <section data-testid="redeem-screen" className="flex w-full flex-col gap-4">
      <header className="flex flex-col gap-1.5">
        <h1 className="type-screen-title">{t('redeem.title')}</h1>
        {!redeem.isSuccess && (
          <p className="type-body" style={{ color: 'var(--ink-2)', textWrap: 'pretty' }}>
            {t('redeem.intro')}
          </p>
        )}
      </header>

      {redeem.isSuccess ? (
        <Done
          voucherId={redeem.data.voucher_id}
          amount={formatMoney(redeem.data.amount, redeem.data.currency)}
          onAnother={startOver}
        />
      ) : (
        <>
          <form
            data-testid="redeem-entry"
            className="flex flex-col gap-3"
            onSubmit={(event: FormEvent) => {
              event.preventDefault()
              submit(typed)
            }}
          >
            <Button
              variant="secondary"
              size="lg"
              data-testid="redeem-scan"
              className="w-full"
              disabled={lookup.isPending}
              onClick={() => {
                setCameraUnavailable(false)
                setScanning((open) => !open)
              }}
            >
              {scanning ? <CameraOff aria-hidden size={18} /> : <Camera aria-hidden size={18} />}
              {scanning ? t('redeem.stopScan') : t('redeem.scan')}
            </Button>

            {scanning && (
              <QrScanner
                onResult={(text) => {
                  setScanning(false)
                  submit(text)
                }}
                onUnavailable={() => {
                  setScanning(false)
                  setCameraUnavailable(true)
                }}
              />
            )}

            {cameraUnavailable && (
              <p
                data-testid="redeem-camera-unavailable"
                role="status"
                className="type-body px-3.5 py-2.5"
                style={{
                  border: '1px solid var(--rule-2)',
                  borderRadius: 'var(--radius-control)',
                  background: 'var(--sand-2)',
                  color: 'var(--ink-2)',
                }}
              >
                {t('redeem.cameraUnavailable')}
              </p>
            )}

            <div className="flex flex-col gap-1.5">
              <label htmlFor="redeem-code" className="type-small" style={{ fontWeight: 600, color: 'var(--ink-2)' }}>
                {t('redeem.code')}
              </label>
              <div className="flex flex-wrap gap-2">
                <input
                  id="redeem-code"
                  data-testid="redeem-code"
                  value={typed}
                  onChange={(event) => setTyped(event.target.value)}
                  // A voucher code is cash. The browser must not offer it back
                  // to the next person at this phone.
                  autoComplete="off"
                  autoCorrect="off"
                  autoCapitalize="characters"
                  spellCheck={false}
                  enterKeyHint="search"
                  className="tabular"
                  style={{ ...CONTROL_FIELD, flex: '1 1 200px', minWidth: 0, letterSpacing: '0.08em' }}
                />
                <Button
                  type="submit"
                  size="lg"
                  data-testid="redeem-lookup"
                  className="w-full sm:w-auto"
                  disabled={lookup.isPending}
                >
                  {lookup.isPending ? t('redeem.lookingUp') : t('redeem.lookUp')}
                </Button>
              </div>
            </div>

            {notAVoucher && (
              <p
                data-testid="redeem-not-a-voucher"
                role="status"
                className="type-body-strong"
                style={{ color: 'var(--flag-ink)' }}
              >
                {t('redeem.notAVoucher')}
              </p>
            )}
          </form>

          {lookup.isError && <ErrorState error={lookup.error} />}

          {lookup.isSuccess &&
            code &&
            (lookup.data.found ? (
              <Preview
                code={code}
                voucher={lookup.data}
                idType={idType}
                onIdType={setIdType}
                nameConfirmed={nameConfirmed}
                onNameConfirmed={setNameConfirmed}
                pending={redeem.isPending}
                error={redeem.error}
                onConfirm={confirm}
              />
            ) : (
              // Unknown, or outside this officer's villages: the same answer on
              // purpose. It is an answer, not a failure.
              <div data-testid="redeem-not-found">
                <EmptyState title={t('redeem.notFound')} />
              </div>
            ))}
        </>
      )}
    </section>
  )
}

function Preview({
  code,
  voucher,
  idType,
  onIdType,
  nameConfirmed,
  onNameConfirmed,
  pending,
  error,
  onConfirm,
}: {
  code: string
  voucher: VoucherPreview
  idType: IdDocumentType | ''
  onIdType: (value: IdDocumentType) => void
  nameConfirmed: boolean
  onNameConfirmed: (value: boolean) => void
  pending: boolean
  error: Error | null
  onConfirm: () => void
}) {
  const { t, i18n } = useTranslation()
  const amount = formatMoney(voucher.amount, voucher.currency)

  return (
    <article
      data-testid="redeem-preview"
      className="flex flex-col gap-4 p-4"
      style={{
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-card)',
        background: 'var(--paper)',
      }}
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="type-body-strong tabular" translate="no" style={{ letterSpacing: '0.06em' }}>
          {formatVoucherCode(code)}
        </span>
        <StatusPill kind="voucher" status={voucher.expired ? 'expired' : voucher.status} />
      </div>

      <div className="flex flex-col gap-0.5">
        <p className="type-note" style={{ color: 'var(--ink-2)' }}>
          {t('redeem.amount')}
        </p>
        <p data-testid="redeem-amount" className="type-figure tabular">
          {amount}
        </p>
        <p className="type-note tabular" style={{ color: 'var(--ink-3)' }}>
          {t('redeem.expires', { date: formatTimestamp(voucher.expires_at) })}
        </p>
      </div>

      <dl className="flex flex-col gap-2.5">
        <Fact testId="redeem-household" label={t('redeem.household')} value={voucher.household_label} />
        <Fact testId="redeem-head" label={t('redeem.head')} value={voucher.head_name} />
        <Fact testId="redeem-respondent" label={t('redeem.respondent')} value={voucher.respondent_name} />
        <Fact
          testId="redeem-survey"
          label={t('redeem.survey')}
          value={localisedField(voucher, 'survey_title', i18n.resolvedLanguage)}
        />
      </dl>

      {voucher.needs_ops && (
        <p
          data-testid="redeem-needs-ops"
          className="type-body-strong px-3.5 py-2.5"
          style={{
            border: '1px solid var(--rule-2)',
            borderLeft: '4px solid var(--primary)',
            borderRadius: 'var(--radius-control)',
            background: 'var(--primary-tint)',
            color: 'var(--primary-ink)',
          }}
        >
          {t('redeem.needsOps')}
        </p>
      )}

      {voucher.can_redeem ? (
        <form
          className="flex flex-col gap-3.5"
          onSubmit={(event: FormEvent) => {
            event.preventDefault()
            onConfirm()
          }}
        >
          <div className="flex flex-col gap-1.5">
            <label htmlFor="redeem-id-type" className="type-small" style={{ fontWeight: 600, color: 'var(--ink-2)' }}>
              {t('redeem.idType')}
            </label>
            <Select value={idType} onValueChange={(value) => onIdType(value as IdDocumentType)}>
              <SelectTrigger id="redeem-id-type" data-testid="redeem-id-type" className="min-h-12 text-base">
                {idType ? t(`auditTrail.idType.${idType}`) : t('redeem.idTypePlaceholder')}
              </SelectTrigger>
              <SelectContent>
                {ID_TYPES.map((type) => (
                  <SelectItem key={type} value={type}>
                    {t(`auditTrail.idType.${type}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <label className="flex items-start gap-3" style={{ minHeight: 48, cursor: 'pointer' }}>
            <input
              type="checkbox"
              data-testid="redeem-name-confirm"
              checked={nameConfirmed}
              onChange={(event) => onNameConfirmed(event.target.checked)}
              style={{ width: 22, height: 22, marginTop: 1, flex: 'none', accentColor: 'var(--primary)' }}
            />
            <span className="type-body">{t('redeem.nameConfirm')}</span>
          </label>

          {/* The database's refusal, as written; the preview stays so the
              officer can correct what it names and try again. */}
          {error && <ErrorState error={error} />}

          <Button type="submit" size="lg" data-testid="redeem-confirm" className="w-full" disabled={pending}>
            {pending ? t('redeem.confirming') : t('redeem.confirm', { amount })}
          </Button>
        </form>
      ) : (
        <div
          data-testid="redeem-blocked"
          className="flex flex-col gap-1 px-3.5 py-3"
          style={{
            border: '1px solid var(--rule-2)',
            borderLeft: '4px solid var(--flag-ink)',
            borderRadius: 'var(--radius-control)',
            background: 'var(--flag-tint)',
          }}
        >
          <h2 className="type-body-strong" style={{ color: 'var(--flag-ink)' }}>
            {t('redeem.cannotRedeem')}
          </h2>
          {voucher.blocked_reason && (
            <p className="type-body" style={{ color: 'var(--ink)', textWrap: 'pretty' }}>
              {dbReasonText(t, voucher.blocked_reason)}
            </p>
          )}
        </div>
      )}
    </article>
  )
}

function Fact({ testId, label, value }: { testId: string; label: string; value: string | null }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="type-note" style={{ color: 'var(--ink-2)' }}>
        {label}
      </dt>
      <dd data-testid={testId} className="type-body-strong">
        {value || '—'}
      </dd>
    </div>
  )
}

function Done({ voucherId, amount, onAnother }: { voucherId: string; amount: string; onAnother: () => void }) {
  const { t } = useTranslation()
  const timeline = useVoucherTimeline(voucherId)

  return (
    <div data-testid="redeem-done" className="flex flex-col gap-4">
      <div
        role="status"
        className="flex flex-col gap-1 px-4 py-3.5"
        style={{
          border: '1px solid var(--green-ink)',
          borderLeft: '4px solid var(--green-ink)',
          borderRadius: 'var(--radius-card)',
          background: 'var(--green-tint)',
        }}
      >
        <h2 className="type-title" style={{ color: 'var(--green-ink)' }}>
          {t('redeem.done')}
        </h2>
        <p className="type-body-strong tabular" style={{ color: 'var(--ink)' }}>
          {t('redeem.doneDetail', { amount })}
        </p>
      </div>

      <section className="flex flex-col gap-2.5">
        <h3 className="type-section" style={{ color: 'var(--ink-3)' }}>
          {t('auditTrail.title')}
        </h3>
        {timeline.isError ? (
          <ErrorState error={timeline.error} onRetry={() => void timeline.refetch()} />
        ) : timeline.data ? (
          <AuditTimeline events={timeline.data} />
        ) : (
          <Loading testId="redeem-timeline-loading" />
        )}
      </section>

      <Button size="lg" data-testid="redeem-another" className="w-full" onClick={onAnother}>
        {t('redeem.another')}
      </Button>
    </div>
  )
}

