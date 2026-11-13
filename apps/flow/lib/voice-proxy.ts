import "server-only"

import { exactOrigin } from "./links"

/**
 * Flow's restricted way to Office's voice server (apps/office/app/api/voice/*), over the
 * internal network. The browser only ever picks one name from ROUTES; the upstream origin
 * comes from the server-only VOICE_UPSTREAM_URL (compose: http://office:3000), read per request.
 *
 * Not forwarded, on purpose:
 * - `secure`: its only caller is Office's own secure form (apps/office/components/voice/
 *   secure-form.tsx), and the handoff it serves points at Office pages.
 * - Upstream Set-Cookie: the only one is the gap cookie (apps/office/lib/voice/gaps.ts),
 *   read only by Office's page proxy on the next Office page (gap-follow-up.ts); on Flow's
 *   origin nothing reads it, and browser cookies never go upstream anyway.
 */
const ROUTES: Record<string, Partial<Record<string, number>>> = {
  readiness: { GET: 10_000 },
  history: { POST: 60_000 },
  session: { POST: 120_000, DELETE: 60_000 },
  tools: { POST: 120_000 },
  batch: { POST: 120_000 },
  // A confirmed fixed batch runs all its writes (up to 25) inside this one request.
  confirm: { POST: 600_000 },
  attachments: { POST: 120_000, DELETE: 60_000 },
}

/** Office's largest accepted body: multipart 10 MiB + 64 KiB (apps/office/lib/voice/input.ts). */
export const UPSTREAM_BODY_LIMIT = 10 * 1024 * 1024 + 65536

/** Browser headers Office's voice routes read; everything else (cookies, Host, x-forwarded-*, hop-by-hop) stays here. */
const REQUEST_HEADERS = [
  "authorization",
  "content-type",
  "content-length",
  "accept",
  "x-workspace-id",
  "x-voice-session",
  "x-file-name",
]
const RESPONSE_HEADERS = ["content-type", "cache-control", "retry-after"]

const failure = (status: number, message: string, headers?: HeadersInit) =>
  Response.json(
    { message },
    { status, headers: { "Cache-Control": "no-store", ...headers } }
  )
const unavailable = () =>
  failure(502, "The assistant service is not available right now. Try again.")

class TooLarge extends Error {}

/** The body as it streams through: never more than `limit`, and exactly `declared` when set. */
function bounded(
  body: ReadableStream<Uint8Array>,
  declared: number | undefined
): ReadableStream<Uint8Array> {
  let seen = 0
  const limit = declared ?? UPSTREAM_BODY_LIMIT
  return body.pipeThrough(
    new TransformStream<Uint8Array, Uint8Array>({
      transform(chunk, controller) {
        seen += chunk.byteLength
        if (seen > limit) controller.error(new TooLarge())
        else controller.enqueue(chunk)
      },
      flush(controller) {
        if (declared !== undefined && seen !== declared)
          controller.error(new TooLarge())
      },
    })
  )
}

const warn = (route: string, kind: string) =>
  console.warn(`voice proxy ${route} ${kind}`)

export async function forwardVoice(
  request: Request,
  route: string,
  {
    upstream = process.env.VOICE_UPSTREAM_URL,
    timeoutMs,
  }: { upstream?: string; timeoutMs?: number } = {}
): Promise<Response> {
  const methods = Object.hasOwn(ROUTES, route) ? ROUTES[route]! : undefined
  if (!methods) return failure(404, "This assistant address does not exist.")
  const timeout = methods[request.method]
  if (timeout === undefined)
    return failure(405, "This assistant address does not take that request.", {
      Allow: Object.keys(methods).join(", "),
    })
  const base = exactOrigin(upstream)
  if (!base) {
    warn(route, "not_configured")
    return unavailable()
  }

  const headers = new Headers()
  for (const name of REQUEST_HEADERS) {
    const value = request.headers.get(name)
    if (value !== null) headers.set(name, value)
  }
  const length = request.headers.get("content-length")
  const declared = length === null ? undefined : Number(length)
  if (
    declared !== undefined &&
    (!Number.isSafeInteger(declared) ||
      declared < 0 ||
      declared > UPSTREAM_BODY_LIMIT)
  ) {
    warn(route, "too_large")
    return failure(413, "This request is too large.")
  }

  const body = request.body ? bounded(request.body, declared) : null
  const deadline = AbortSignal.timeout(timeoutMs ?? timeout)
  let response: Response
  try {
    response = await fetch(new URL(`/api/voice/${route}`, base), {
      method: request.method,
      headers,
      body,
      duplex: "half",
      redirect: "manual",
      cache: "no-store",
      signal: AbortSignal.any([request.signal, deadline]),
    } as RequestInit)
  } catch (error) {
    if (hasCause(error, TooLarge)) {
      warn(route, "too_large")
      return failure(413, "This request is too large.")
    }
    if (deadline.aborted) {
      warn(route, "timeout")
      return failure(
        504,
        "The assistant service took too long to answer. Try again."
      )
    }
    warn(route, request.signal.aborted ? "client_gone" : "unreachable")
    return unavailable()
  }

  const empty =
    response.status === 204 ||
    response.status === 205 ||
    response.headers.get("content-length") === "0"
  const json = /^application\/json\b/i.test(
    response.headers.get("content-type") ?? ""
  )
  if ((response.status >= 300 && response.status < 400) || (!empty && !json)) {
    await response.body?.cancel().catch(() => undefined)
    warn(route, "invalid_response")
    return unavailable()
  }
  const out = new Headers()
  for (const name of RESPONSE_HEADERS) {
    const value = response.headers.get(name)
    if (value !== null) out.set(name, value)
  }
  return new Response(empty ? null : response.body, {
    status: response.status,
    headers: out,
  })
}

function hasCause(error: unknown, type: typeof TooLarge): boolean {
  for (let e = error, i = 0; e && i < 5; e = (e as Error).cause, i++)
    if (e instanceof type) return true
  return false
}
