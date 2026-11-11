export type SceneError =
  | { kind: "unauthorized" | "forbidden" | "missing" | "transient" | "other" }
  | { kind: "busy"; retryAfter: number }

type Failure = {
  response?: { status?: number; headers?: Record<string, unknown> }
}

/** What a failed scene call means for the screen, from the HTTP answer (none = network trouble). */
export function sceneError(error: unknown): SceneError {
  const res = (error as Failure | null)?.response
  const status = res?.status
  if (status === 401) return { kind: "unauthorized" }
  if (status === 403) return { kind: "forbidden" }
  if (status === 404) return { kind: "missing" }
  if (status === 429) {
    const secs = Number(res?.headers?.["retry-after"])
    return {
      kind: "busy",
      retryAfter: Number.isFinite(secs) && secs >= 0 ? Math.min(secs, 60) : 5,
    }
  }
  if (!status || status === 408 || status >= 500) return { kind: "transient" }
  return { kind: "other" }
}

/** Only trouble that can pass is retried (network, 5xx, 408, 429); a "no" stays a "no". */
export const shouldRetry = (failures: number, error: unknown) =>
  failures < 3 && ["transient", "busy"].includes(sceneError(error).kind)

export function retryDelay(failures: number, error: unknown) {
  const e = sceneError(error)
  return e.kind === "busy"
    ? e.retryAfter * 1000
    : Math.min(1000 * 2 ** failures, 10_000)
}
