/**
 * Reading a failed Genesy API call (an axios error from the generated client).
 * Bodies are `{message}` or, on 422, `{message, errors: {field: [text]}}`; the first field text is
 * the most specific, so it wins. Messages are shown as written (or translated by dbMessages).
 */
type ApiBody = { message?: string; errors?: Record<string, string[] | undefined> }
type MaybeAxios = { isAxiosError?: boolean; response?: { status?: number; data?: ApiBody } }

/** HTTP status, or undefined when the request never got an answer (offline, refused). */
export function apiStatus(error: unknown): number | undefined {
  return (error as MaybeAxios | null)?.response?.status
}

/** The request was sent and nothing came back: offline, timed out, connection dropped. */
export function isNoAnswer(error: unknown): boolean {
  return (error as MaybeAxios | null)?.isAxiosError === true && apiStatus(error) === undefined
}

export function apiErrorMessage(error: unknown): string | null {
  const body = (error as MaybeAxios | null)?.response?.data
  const first = Object.values(body?.errors ?? {}).find((list) => list?.length)?.[0]
  return first ?? body?.message ?? null
}

/** The form field an API key belongs to: a list of keys that are field names as they are, or a mapping (null: no field). */
export type FieldOf = readonly string[] | ((key: string) => string | null | undefined)

/**
 * A 422's `errors` split between the form's fields and the general error Notice. `fields` holds the
 * first text per form field; `general` is what the Notice still has to say: the error itself when no
 * text landed on a field (any other status too), the first text whose key has no field, or null.
 */
export function fieldErrorsOf(error: unknown, fieldOf: FieldOf): { fields: Record<string, string>; general: unknown } {
  const errors = apiStatus(error) === 422 ? (error as MaybeAxios).response?.data?.errors ?? {} : {}
  const map = typeof fieldOf === 'function' ? fieldOf : (key: string) => (fieldOf.includes(key) ? key : null)
  const fields: Record<string, string> = {}
  let left: string | null = null
  for (const [key, texts] of Object.entries(errors)) {
    const text = texts?.[0]
    if (!text) continue
    const field = map(key)
    if (field) fields[field] ??= text
    else left ??= text
  }
  return { fields, general: Object.keys(fields).length === 0 ? (error ?? null) : left }
}
