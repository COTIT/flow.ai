import { useRef, useState } from 'react'
import { z } from 'zod'
import { zodResolver } from '@hookform/resolvers/zod'
import { useTranslation } from 'react-i18next'

import { useWorkspaceId } from '@/app/workspace'
import { ErrorState } from '@/components/ErrorState'
import { FormField as Field } from '@/components/FormField'
import { CONTROL } from '@/components/controlStyles'
import { Button } from '@/components/ui/button'
import { useCreateCategory, useEquipmentCategories } from '@/features/ops/useCatalogue'
import { finishDraftWhenSaved } from '@/lib/drafts'
import { usePersistentForm } from '@/lib/usePersistentForm'

const schema = z.object({ code: z.string(), nameEn: z.string(), nameSw: z.string() })
const DEFAULTS = { code: '', nameEn: '', nameSw: '' }

/**
 * Equipment categories: the list a machine is filed under, and a small form to add one. Both
 * names are required (reference data is translated by whoever enters it); a unique code is the
 * API's rule, and its message is shown.
 */
export function CatalogueCategories() {
  const { t } = useTranslation()
  const categories = useEquipmentCategories()
  const create = useCreateCategory()
  const draft = usePersistentForm('catalogue-category-create', useWorkspaceId() ?? 'workspace', DEFAULTS, zodResolver(schema))
  const [code, setCode] = draft.field('code')
  const [nameEn, setNameEn] = draft.field('nameEn')
  const [nameSw, setNameSw] = draft.field('nameSw')
  const [touched, setTouched] = useState(false)
  // isPending is only true on the next render; a ref latches in the same tick.
  const inFlight = useRef(false)

  const errors = { code, 'name-en': nameEn, 'name-sw': nameSw }
  const missing = (key: keyof typeof errors) => touched && errors[key].trim() === ''

  const submit = () => {
    if (inFlight.current || create.isPending || !draft.ready) return
    setTouched(true)
    if (Object.values(errors).some((v) => v.trim() === '')) return
    inFlight.current = true
    void finishDraftWhenSaved(
      create.mutateAsync({ id: draft.clientRef, code: code.trim(), name_en: nameEn.trim(), name_sw: nameSw.trim() }),
      draft.finish,
    ).finally(() => {
      inFlight.current = false
      setTouched(false)
    })
  }

  const input = (id: keyof typeof errors, value: string, set: (v: string) => void) => (
    <Field
      label={t(id === 'code' ? 'catalogue.code' : id === 'name-en' ? 'catalogue.nameEn' : 'catalogue.nameSw')}
      id={`category-${id}`}
      error={missing(id) ? t('catalogue.required') : undefined}
      errorTestId={`category-${id}-error`}
    >
      <input
        id={`category-${id}`}
        data-testid={`category-${id}`}
        value={value}
        onChange={(e) => set(e.target.value)}
        className="w-full"
        style={CONTROL}
      />
    </Field>
  )

  return (
    <section
      data-testid="catalogue-categories"
      className="flex w-full flex-col gap-3 p-4 sm:p-[18px]"
      style={{ border: '1px solid var(--rule)', borderRadius: 'var(--radius-card)', background: 'var(--paper)' }}
    >
      <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>{t('catalogue.categories.title')}</h2>
      {categories.error ? (
        <ErrorState error={categories.error} onRetry={() => void categories.refetch()} />
      ) : (
        <ul className="flex flex-wrap gap-2">
          {(categories.data ?? []).map((c) => (
            <li
              key={c.id}
              data-testid="category-row"
              className="inline-flex items-baseline gap-2 px-3 py-1.5"
              style={{ border: '1px solid var(--rule-2)', borderRadius: 'var(--radius-pill)', fontSize: 14 }}
            >
              {c.name}
              <span className="type-note tabular" style={{ color: 'var(--ink-3)' }}>{c.code}</span>
            </li>
          ))}
        </ul>
      )}
      <form
        className="flex flex-col gap-3"
        noValidate
        onSubmit={(e) => {
          e.preventDefault()
          submit()
        }}
      >
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          {input('code', code, setCode)}
          {input('name-en', nameEn, setNameEn)}
          {input('name-sw', nameSw, setNameSw)}
        </div>
        {create.isError && (
          <div data-testid="category-create-error">
            <ErrorState error={create.error} onRetry={() => create.reset()} />
          </div>
        )}
        <Button type="submit" data-testid="category-submit" disabled={!draft.ready || create.isPending} className="w-fit">
          {create.isPending ? t('catalogue.adding') : t('catalogue.categories.add')}
        </Button>
      </form>
    </section>
  )
}
