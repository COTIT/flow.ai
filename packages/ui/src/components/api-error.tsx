import { Notice } from "@genesy/ui/components/notice"

/** The API's error body: `message`, plus `errors` per field on a 422. */
type ErrorBody = { message?: string; errors?: Record<string, string[]> }

function body(error: unknown): ErrorBody | undefined {
  return (error as { response?: { data?: ErrorBody } } | null)?.response?.data
}

/** Field → messages from a 422, to show next to each input. */
function fieldErrors(error: unknown): Record<string, string[]> {
  return body(error)?.errors ?? {}
}

/**
 * An error Notice with the API's own words for a failed call; `fallback` when it sent none.
 * `codes` turns a 409's code (`lot_recalled`) into the caller's sentence, so no code is shown.
 */
function ApiError({
  error,
  fallback,
  codes,
}: {
  error: unknown
  fallback: string
  codes?: Record<string, string>
}) {
  if (!error) return null
  const message = body(error)?.message
  return (
    <Notice
      variant="error"
      title={(message && codes?.[message]) ?? message ?? fallback}
    />
  )
}

export { ApiError, fieldErrors }
