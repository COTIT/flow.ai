/**
 * Reading a failed Genesy API call (an axios error from the generated client).
 * Bodies are `{message}` or, on 422, `{message, errors: {field: [text]}}`; the first field text is
 * the most specific, so it wins. Messages are shown as written (or translated by dbMessages).
 */
type ApiBody = { message?: string; errors?: Record<string, string[] | undefined> }
type MaybeAxios = { response?: { status?: number; data?: ApiBody } }

/** HTTP status, or undefined when the request never got an answer (offline, refused). */
export function apiStatus(error: unknown): number | undefined {
  return (error as MaybeAxios | null)?.response?.status
}

export function apiErrorMessage(error: unknown): string | null {
  const body = (error as MaybeAxios | null)?.response?.data
  const first = Object.values(body?.errors ?? {}).find((list) => list?.length)?.[0]
  return first ?? body?.message ?? null
}
