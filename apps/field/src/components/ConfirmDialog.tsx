import { useEffect, useId, useRef } from 'react'

/** Small, accessible confirmation surface for irreversible record actions. */
export function ConfirmDialog({
  title,
  detail,
  confirmLabel,
  cancelLabel,
  confirming = false,
  onConfirm,
  onCancel,
}: {
  title: string
  detail: string
  confirmLabel: string
  cancelLabel: string
  confirming?: boolean
  onConfirm: () => void
  onCancel: () => void
}) {
  const titleId = useId()
  const detailId = useId()
  const confirmRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    confirmRef.current?.focus()
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !confirming) onCancel()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [confirming, onCancel])

  return (
    <div
      data-testid="confirm-dialog-backdrop"
      className="fixed inset-0 z-50 flex items-end justify-center p-4 pb-[max(1rem,env(safe-area-inset-bottom))] sm:items-center"
      style={{ background: 'rgba(18, 31, 42, .42)' }}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !confirming) onCancel()
      }}
    >
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={detailId}
        data-testid="confirm-dialog"
        className="flex w-full max-w-md flex-col gap-3.5 p-5"
        style={{
          border: '1px solid var(--rule)',
          borderRadius: 'var(--radius-card)',
          background: 'var(--paper)',
          boxShadow: '0 18px 50px rgba(18, 31, 42, .22)',
        }}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <h2 id={titleId} style={{ fontSize: 18, fontWeight: 700 }}>
          {title}
        </h2>
        <p id={detailId} style={{ fontSize: 14, lineHeight: 1.55, color: 'var(--ink-2)' }}>
          {detail}
        </p>
        <div className="flex flex-wrap justify-end gap-2.5">
          <button
            type="button"
            data-testid="confirm-dialog-cancel"
            disabled={confirming}
            onClick={onCancel}
            className="px-4 font-semibold disabled:opacity-60"
            style={{
              minHeight: 48,
              border: '1px solid var(--rule-2)',
              borderRadius: 'var(--radius-control)',
              background: 'var(--paper)',
              color: 'var(--ink)',
            }}
          >
            {cancelLabel}
          </button>
          <button
            ref={confirmRef}
            type="button"
            data-testid="confirm-dialog-confirm"
            disabled={confirming}
            onClick={onConfirm}
            className="px-4 font-semibold disabled:opacity-60"
            style={{
              minHeight: 48,
              border: '1.5px solid var(--primary)',
              borderRadius: 'var(--radius-control)',
              background: 'var(--primary)',
              color: '#fff',
            }}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  )
}
