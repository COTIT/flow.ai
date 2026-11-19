import { Notice } from "@verzo/ui/components/notice"

/** The API's error body: `message`, plus `errors` per field on a 422. */
type ErrorBody = {
  message?: string
  code?: string
  errors?: Record<string, string[]>
}

function body(error: unknown): ErrorBody | undefined {
  return (error as { response?: { data?: ErrorBody } } | null)?.response?.data
}

/** Field → messages from a 422, to show next to each input. */
function fieldErrors(error: unknown): Record<string, string[]> {
  return body(error)?.errors ?? {}
}

/**
 * An error Notice with the API's own words for a failed call; `fallback` when it sent none.
 * `codes` turns an error's code (`lot_recalled` as the message, or a `code` field such as
 * `stale_document`) into the caller's sentence, so no code is shown.
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
  const { message, code } = body(error) ?? {}
  return (
    <Notice
      variant="error"
      title={
        (code && codes?.[code]) ??
        (message && codes?.[message]) ??
        message ??
        fallback
      }
    />
  )
}

export { ApiError, fieldErrors }
