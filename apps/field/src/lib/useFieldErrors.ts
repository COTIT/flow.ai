import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'

import { fieldErrorsOf, type FieldOf } from '@/lib/apiError'
import { errorText } from '@/lib/errors'

/** react-hook-form's `setError`, for any form's field names. */
type SetError = (name: never, error: { type: string; message: string }, options?: { shouldFocus: boolean }) => void

/**
 * A refused save's 422 texts, each on its react-hook-form field (humanised like every API text), the
 * first one focused: by `idOf` when the inputs are not registered with the form, else by the form.
 * Returns what is left for the general error Notice (`fieldErrorsOf`'s `general`). Each refusal is
 * set once, so an edit that clears a field's error is not undone by the next render.
 */
export function useFieldErrors(error: unknown, setError: SetError, fieldOf: FieldOf, idOf?: (field: string) => string): unknown {
  const { t } = useTranslation()
  const { fields, general } = fieldErrorsOf(error, fieldOf)
  const placed = useRef<unknown>(null)

  useEffect(() => {
    if (!error || placed.current === error) return
    placed.current = error
    const set = setError as unknown as (name: string, error: { type: string; message: string }, options: { shouldFocus: boolean }) => void
    Object.entries(fields).forEach(([field, text], index) => {
      set(field, { type: 'server', message: errorText(text, t) }, { shouldFocus: index === 0 && !idOf })
      if (index === 0 && idOf) document.getElementById(idOf(field))?.focus()
    })
  })

  return general
}
