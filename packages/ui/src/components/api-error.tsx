/** The API's error body: `message`, plus `errors` per field on a 422. */
type ErrorBody = { message?: string; errors?: Record<string, string[]> }

function body(error: unknown): ErrorBody | undefined {
  return (error as { response?: { data?: ErrorBody } } | null)?.response?.data
}

/** Field → messages from a 422, to show next to each input. */
function fieldErrors(error: unknown): Record<string, string[]> {
  return body(error)?.errors ?? {}
}

/** Shows the API's own words for a failed call; `fallback` when it sent none. */
function ApiError({ error, fallback }: { error: unknown; fallback: string }) {
  if (!error) return null
  return (
    <p role="alert" className="text-sm text-destructive">
      {body(error)?.message ?? fallback}
    </p>
  )
}

export { ApiError, fieldErrors }
