import { Drawer } from '@base-ui/react/drawer'
import { useMutation } from '@tanstack/react-query'
import { Bug } from 'lucide-react'
import { useId, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { BUTTON_PRIMARY, BUTTON_SECONDARY, CONTROL_FIELD } from '@/components/controlStyles'
import { FormField } from '@/components/FormField'
import { captureScreen, submitBugReport } from '@/lib/bugReports'
import type { TrailEntry } from '@/lib/bugTrail'
import { trailSnapshot } from '@/lib/bugTrail'

/**
 * The floating "report a problem" button and its bottom sheet (migration
 * 20261025090001_bug_reports).
 *
 * The tap freezes the last 30 steps and takes the screenshot BEFORE the sheet
 * opens, so neither the button nor the sheet is in the picture and the sheet's
 * own clicks are not on the trail. Everything inside carries
 * `data-bug-report`, which both the trail and the screenshot skip.
 */
export function BugReportButton({ reporterId }: { reporterId: string }) {
  const { t } = useTranslation()
  const [capturing, setCapturing] = useState(false)
  const [captured, setCaptured] = useState<{ actions: TrailEntry[]; screenshot: Blob | null } | null>(null)
  const [sent, setSent] = useState(false)

  const open = async () => {
    if (capturing) return
    setSent(false)
    setCapturing(true)
    const actions = trailSnapshot()
    const screenshot = await captureScreen()
    setCaptured({ actions, screenshot })
    setCapturing(false)
  }

  return (
    <>
      <button
        type="button"
        data-bug-report
        data-testid="bug-fab"
        aria-label={capturing ? t('bugReport.capturing') : t('bugReport.open')}
        aria-busy={capturing}
        onClick={open}
        className="fixed right-4 z-20 flex size-14 items-center justify-center rounded-full disabled:opacity-70 mr-[env(safe-area-inset-right)]"
        style={{
          bottom: 'max(calc(var(--tab-bar-height, 0px) + 12px), calc(env(safe-area-inset-bottom, 0px) + 16px))',
          background: 'var(--primary)',
          color: '#fff',
          border: '2px solid var(--paper)',
        }}
      >
        <Bug aria-hidden />
      </button>

      {sent && (
        <p
          role="status"
          data-bug-report
          data-testid="bug-sent"
          className="fixed inset-x-4 z-20 mx-auto max-w-md rounded-[var(--radius-card)] px-4 py-3 text-center font-medium"
          style={{
            bottom: 'max(calc(var(--tab-bar-height, 0px) + 84px), calc(env(safe-area-inset-bottom, 0px) + 88px))',
            background: 'var(--ink)',
            color: '#fff',
          }}
        >
          {t('bugReport.sent')}
        </p>
      )}

      <Drawer.Root open={captured !== null} onOpenChange={(next) => !next && setCaptured(null)}>
        <Drawer.Portal>
          <Drawer.Backdrop
            data-bug-report
            className="fixed inset-0 z-50"
            style={{ background: 'rgba(18, 31, 42, .42)' }}
          />
          <Drawer.Viewport data-bug-report className="fixed inset-0 z-50 flex items-end justify-center">
            <Drawer.Popup
              data-bug-report
              data-testid="bug-sheet"
              className="max-h-[92dvh] w-full max-w-lg overflow-y-auto overscroll-contain p-5 pb-[max(1.25rem,env(safe-area-inset-bottom))] outline-none [transform:translateY(var(--drawer-swipe-movement-y,0))]"
              style={{
                background: 'var(--paper)',
                borderRadius: 'var(--radius-card) var(--radius-card) 0 0',
                borderTop: '1px solid var(--rule)',
              }}
            >
              <div aria-hidden className="mx-auto mb-4 h-1.5 w-10 rounded-full" style={{ background: 'var(--rule-2)' }} />
              {captured && (
                <BugReportForm
                  reporterId={reporterId}
                  actions={captured.actions}
                  screenshot={captured.screenshot}
                  onSent={() => {
                    setCaptured(null)
                    setSent(true)
                    setTimeout(() => setSent(false), 5000)
                  }}
                />
              )}
            </Drawer.Popup>
          </Drawer.Viewport>
        </Drawer.Portal>
      </Drawer.Root>
    </>
  )
}

/** A preview link for a picked or captured image. */
function useObjectUrl(blob: Blob | null) {
  // shortcut: never revoked (StrictMode's second effect run revoked the live
  // link); one image per report attempt until reload. Revoke if reports get frequent.
  return useMemo(() => (blob ? URL.createObjectURL(blob) : null), [blob])
}

function BugReportForm({
  reporterId,
  actions,
  screenshot,
  onSent,
}: {
  reporterId: string
  actions: TrailEntry[]
  screenshot: Blob | null
  onSent: () => void
}) {
  const { t } = useTranslation()
  const descriptionId = useId()
  const photoId = useId()
  const [description, setDescription] = useState('')
  const [photo, setPhoto] = useState<File | null>(null)
  const [attempted, setAttempted] = useState(false)
  const screenshotUrl = useObjectUrl(screenshot)
  const photoUrl = useObjectUrl(photo)

  const send = useMutation({
    mutationFn: () => submitBugReport({ reporterId, description: description.trim(), actions, screenshot, photo }),
    onSuccess: onSent,
  })

  const descriptionError = attempted && !description.trim() ? t('bugReport.descriptionRequired') : undefined
  const imageError = attempted && !screenshot && !photo ? t('bugReport.imageRequired') : undefined

  return (
    <form
      noValidate
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault()
        setAttempted(true)
        if (description.trim() && (screenshot || photo)) send.mutate()
      }}
    >
      <div className="flex flex-col gap-1">
        <Drawer.Title style={{ fontSize: 18, fontWeight: 700 }}>{t('bugReport.title')}</Drawer.Title>
        <Drawer.Description className="type-note" style={{ color: 'var(--ink-3)' }}>
          {t('bugReport.intro')}
        </Drawer.Description>
      </div>

      {screenshotUrl ? (
        <figure className="flex flex-col gap-1.5">
          <figcaption style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}>
            {t('bugReport.screenshot')}
          </figcaption>
          <img
            data-testid="bug-screenshot"
            src={screenshotUrl}
            alt={t('bugReport.screenshot')}
            className="max-h-56 w-full rounded-[var(--radius-control)] object-contain object-top"
            style={{ border: '1px solid var(--rule)', background: 'var(--sand)' }}
          />
        </figure>
      ) : (
        <p data-testid="bug-screenshot-failed" className="font-medium" style={{ fontSize: 14, color: 'var(--flag-ink)' }}>
          {t('bugReport.screenshotFailed')}
        </p>
      )}

      <FormField label={t('bugReport.photo')} id={photoId} hint={t('bugReport.photoHint')} error={imageError} errorTestId="bug-image-error">
        <input
          id={photoId}
          data-testid="bug-photo"
          type="file"
          accept="image/*"
          style={CONTROL_FIELD}
          onChange={(event) => setPhoto(event.target.files?.[0] ?? null)}
        />
      </FormField>
      {photoUrl && (
        <img
          data-testid="bug-photo-preview"
          src={photoUrl}
          alt={t('bugReport.photo')}
          className="max-h-56 w-full rounded-[var(--radius-control)] object-contain"
          style={{ border: '1px solid var(--rule)' }}
        />
      )}

      <FormField
        label={t('bugReport.description')}
        id={descriptionId}
        hint={t('bugReport.descriptionHint')}
        error={descriptionError}
        errorTestId="bug-description-error"
      >
        <textarea
          id={descriptionId}
          data-testid="bug-description"
          rows={4}
          value={description}
          onChange={(event) => setDescription(event.target.value)}
          style={CONTROL_FIELD}
        />
      </FormField>

      <details data-testid="bug-actions">
        <summary className="cursor-pointer font-medium" style={{ fontSize: 14, color: 'var(--ink-2)' }}>
          {t('bugReport.actions', { count: actions.length })}
        </summary>
        <ol className="mt-2 flex flex-col gap-1 font-mono" style={{ fontSize: 12, color: 'var(--ink-3)' }}>
          {actions.map((entry, i) => (
            <li key={i} className="break-words">
              {entry.at.slice(11, 19)} {entry.kind} {entry.detail}
            </li>
          ))}
        </ol>
      </details>

      {send.error && (
        <p role="alert" data-testid="bug-send-error" className="font-medium" style={{ fontSize: 14, color: 'var(--flag-ink)' }}>
          {send.error.message}
        </p>
      )}

      <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
        <Drawer.Close type="button" style={BUTTON_SECONDARY} disabled={send.isPending}>
          {t('bugReport.cancel')}
        </Drawer.Close>
        <button type="submit" data-testid="bug-submit" style={BUTTON_PRIMARY} disabled={send.isPending}>
          {send.isPending ? t('bugReport.sending') : t('bugReport.submit')}
        </button>
      </div>
    </form>
  )
}
