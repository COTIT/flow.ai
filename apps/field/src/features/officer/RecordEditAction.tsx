import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { OfficerEditDialog, type EditField } from '@/features/officer/OfficerEditDialog'
import { useOfficerEdit } from '@/features/officer/useOfficerEdit'
import { selectedEditMeasure, type CropMeasure, type EditContext, type EditableTable, type EditValues } from '@/features/officer/officerEdit'
import type { Database } from '@/lib/db.types'

export function RecordEditAction({
  table,
  id,
  title,
  fields,
  initialValues,
  context,
  measure,
  measureByCrop,
  version,
}: {
  table: EditableTable
  id: string
  title: string
  fields: EditField[]
  initialValues: EditValues
  context: EditContext
  measure?: Database['public']['Enums']['crop_measure']
  measureByCrop?: Readonly<Record<string, CropMeasure>>
  /** The record's captured_at; see OfficerEditDialog. */
  version?: string | null
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const edit = useOfficerEdit(measure)

  return (
    <>
      <button type="button" data-testid={`edit-${table}-${id}`} onClick={() => { edit.reset(); setOpen(true) }} style={EDIT_BUTTON}>
        {t('officerEdit.action')}
      </button>
      {open && (
        <OfficerEditDialog
          table={table}
          recordId={id}
          title={title}
          fields={fields}
          initialValues={initialValues}
          measure={measure}
          measureByCrop={measureByCrop}
          version={version}
          pending={edit.isPending}
          error={edit.error}
          onSave={(values) => edit.mutateAsync({
            table,
            id,
            values,
            context,
            measure: selectedEditMeasure(table, values, measureByCrop, measure),
          })}
          onComplete={() => setOpen(false)}
          onCancel={() => { edit.reset(); setOpen(false) }}
        />
      )}
    </>
  )
}

const EDIT_BUTTON: React.CSSProperties = {
  minHeight: 40,
  border: '1px solid var(--rule-2)',
  borderRadius: 'var(--radius-control)',
  background: 'var(--paper)',
  padding: '0 12px',
  color: 'var(--primary-ink)',
  fontWeight: 600,
}
