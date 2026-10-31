import { useState } from 'react'
import { Check } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { ConfirmDialog } from '@/components/ConfirmDialog'
import type { VerifiableTable } from '@/features/officer/personDetail'
import type { Database } from '@/lib/db.types'

type Verification = Database['public']['Enums']['verification_status']

/**
 * Verify one record. Officer-only, and the only route to verification is
 * app_verify — nothing sets the columns directly.
 *
 * 48px, because it is tapped in a list, outdoors, one row after another.
 */
export function VerifyButton({
  table,
  id,
  verification,
  recordLabel,
  onVerify,
  pending,
}: {
  table: VerifiableTable
  id: string
  verification: Verification
  recordLabel?: string
  onVerify: (target: { table: VerifiableTable; id: string }) => void | Promise<unknown>
  pending: boolean
}) {
  const { t } = useTranslation()
  const [confirming, setConfirming] = useState(false)
  const [submitted, setSubmitted] = useState(false)

  // Already verified: nothing to do, and no control that implies otherwise.
  if (verification === 'verified') return null

  return (
    <>
      <button
        type="button"
        data-testid={`verify-${table}-${id}`}
        data-verify-table={table}
        disabled={[pending, submitted].some(Boolean)}
        onClick={() => setConfirming(true)}
        className="inline-flex items-center justify-center gap-2 px-4 font-semibold disabled:opacity-60"
        style={{
          minHeight: 48,
          border: '1.5px solid var(--primary)',
          borderRadius: 'var(--radius-control)',
          background: 'var(--primary-tint)',
          color: 'var(--primary-ink)',
          fontSize: 15,
          fontFamily: 'inherit',
          flex: 'none',
        }}
      >
        <Check aria-hidden size={17} strokeWidth={2.5} style={{ flex: 'none' }} />
        {pending ? t('person.verifying') : t('person.verify')}
      </button>
      {confirming && (
        <ConfirmDialog
          title={t('verifyDialog.title')}
          detail={t('verifyDialog.detail', { record: recordLabel ?? table })}
          confirmLabel={pending ? t('person.verifying') : t('verifyDialog.confirm')}
          cancelLabel={t('verifyDialog.cancel')}
          confirming={pending}
          onCancel={() => setConfirming(false)}
          onConfirm={() => {
            setSubmitted(true)
            setConfirming(false)
            // mutateAsync rejects on a server failure, so the same row becomes
            // actionable again without weakening the double-submit guard.
            void Promise.resolve()
              .then(() => onVerify({ table, id }))
              .catch(() => setSubmitted(false))
          }}
        />
      )}
    </>
  )
}
