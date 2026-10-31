import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { z } from 'zod'
import { zodResolver } from '@hookform/resolvers/zod'
import { FormDraftStatus } from '@/components/FormDraftStatus'
import { usePersistentForm } from '@/lib/usePersistentForm'

import { validateEdit, type CropMeasure, type EditableTable, type EditValues } from '@/features/officer/officerEdit'
import type { Database } from '@/lib/db.types'

export interface EditField {
  name: string
  label: string
  type?: 'text' | 'number' | 'date'
  required?: boolean
  step?: string
  options?: ReadonlyArray<{ value: string; label: string }>
}

export function OfficerEditDialog({
  table,
  recordId,
  title,
  fields,
  initialValues,
  measure,
  measureByCrop,
  version,
  pending,
  error,
  onSave,
  onComplete,
  onCancel,
}: {
  table: EditableTable
  recordId?: string
  title: string
  fields: EditField[]
  initialValues: EditValues
  measure?: Database['public']['Enums']['crop_measure']
  measureByCrop?: Readonly<Record<string, CropMeasure>>
  /** The record's captured_at: a draft typed against an older one is discarded. */
  version?: string | null
  pending: boolean
  error: Error | null
  onSave: (values: EditValues) => Promise<unknown> | void
  onComplete?: () => void
  onCancel: () => void
}) {
  // Labels and titles are translation keys; option labels may be DB names.
  const { t } = useTranslation()
  const draft = usePersistentForm<EditValues>('officer-edit', `${table}:${recordId ?? title}`, initialValues, zodResolver(editSchema), { version })
  const values = draft.values
  const [validationError, setValidationError] = useState<string | null>(null)
  const activeMeasure = table === 'crop_cycle'
    ? measureByCrop?.[values.crop_id ?? ''] ?? measure
    : measure
  // Only a crop cycle chooses between measures; a plot's area_ha is plain.
  const visibleFields = fields.filter((field) =>
    table !== 'crop_cycle'
    || !MEASURE_FIELDS.some((name) => name === field.name) || field.name === activeMeasure || (activeMeasure === 'area' && field.name === 'area_ha'),
  )

  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (!draft.ready) return
    const nextError = validateEdit(table, values, activeMeasure)
    if (nextError) {
      setValidationError(nextError)
      return
    }
    setValidationError(null)
    try {
      await onSave(values)
      await draft.finish()
      onComplete?.()
    } catch {
      // The mutation supplies its database error as the `error` prop.
    }
  }

  return (
    <div className="fixed inset-0 z-40 flex items-end justify-center p-4 pb-[max(1rem,env(safe-area-inset-bottom))] sm:items-center" style={{ background: 'rgba(18, 31, 42, .42)' }}>
      <form
        role="dialog"
        aria-modal="true"
        aria-label={t(title)}
        data-testid="officer-edit-dialog"
        noValidate
        onSubmit={submit}
        className="flex max-h-[90dvh] w-full max-w-lg flex-col gap-4 overflow-y-auto p-5"
        style={{
          border: '1px solid var(--rule)',
          borderRadius: 'var(--radius-card)',
          background: 'var(--paper)',
          boxShadow: '0 18px 50px rgba(18, 31, 42, .22)',
        }}
      >
        <h2 style={{ fontSize: 18, fontWeight: 700 }}>{t(title)}</h2>
        <FormDraftStatus dirty={draft.dirty} storageError={draft.storageError} />
        {visibleFields.map((field) => (
          <label key={field.name} className="flex flex-col gap-1.5" style={{ fontSize: 13, fontWeight: 600 }}>
            {t(field.label)}
            {field.options ? (
              <select
                data-testid={`edit-${field.name}`}
                value={values?.[field.name] ?? ''}
                onChange={(event) => draft.field(field.name)[1](event.target.value)}
                disabled={pending}
                style={inputStyle}
              >
                {field.options.map((option) => (
                  <option key={option.value} value={option.value}>{t(option.label, { defaultValue: option.label, nsSeparator: false })}</option>
                ))}
              </select>
            ) : (
              <input
                data-testid={`edit-${field.name}`}
                type={field.type ?? 'text'}
                value={values?.[field.name] ?? ''}
                required={field.required}
                step={field.step}
                onChange={(event) => draft.field(field.name)[1](event.target.value)}
                disabled={pending}
                style={inputStyle}
              />
            )}
          </label>
        ))}
        {(validationError || error) && (
          <p role="alert" data-testid="officer-edit-error" style={{ color: 'var(--flag-ink)', fontSize: 13 }}>
            {error ? error.message : validationError}
          </p>
        )}
        <div className="flex flex-wrap justify-end gap-2.5">
          <button type="button" data-testid="officer-edit-cancel" onClick={onCancel} disabled={pending} style={secondaryStyle}>
            {t('officerEdit.cancel')}
          </button>
          <button type="submit" data-testid="officer-edit-save" disabled={!draft.ready || pending} style={primaryStyle}>
            {pending ? t('officerEdit.saving') : t('officerEdit.save')}
          </button>
        </div>
      </form>
    </div>
  )
}

const editSchema = z.record(z.string(), z.string())

const MEASURE_FIELDS = ['area_ha', 'tree_count', 'unit_count'] as const

const inputStyle: React.CSSProperties = {
  minHeight: 48,
  width: '100%',
  border: '1.5px solid var(--rule-2)',
  borderRadius: 'var(--radius-control)',
  background: 'var(--paper)',
  padding: '12px 14px',
  font: 'inherit',
  fontSize: 16,
  color: 'var(--ink)',
}

const primaryStyle: React.CSSProperties = {
  minHeight: 48,
  border: 0,
  borderRadius: 'var(--radius-control)',
  background: 'var(--primary)',
  padding: '0 18px',
  color: '#fff',
  fontWeight: 600,
}

const secondaryStyle: React.CSSProperties = {
  ...primaryStyle,
  border: '1px solid var(--rule-2)',
  background: 'var(--paper)',
  color: 'var(--ink)',
}
