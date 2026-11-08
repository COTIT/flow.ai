import "server-only"
import type { VoiceContext } from "./server"

/** The workspace's IANA time zone, or undefined (no `setting.get`, unset, invalid); never a guess. */
export async function workspaceTimeZone(
  context: Pick<VoiceContext, "apiUrl" | "token" | "workspaceId">
): Promise<string | undefined> {
  try {
    const response = await fetch(
      `${context.apiUrl}/workspace/${encodeURIComponent(context.workspaceId)}/settings?type=general.timezone`,
      {
        redirect: "error",
        cache: "no-store",
        signal: AbortSignal.timeout(5000),
        headers: {
          Accept: "application/json",
          Authorization: `Bearer ${context.token}`,
        },
      }
    )
    if (!response.ok) return undefined
    const zone = ((await response.json()) as { data?: unknown }).data
    if (typeof zone !== "string" || zone.length > 64) return undefined
    new Intl.DateTimeFormat("en", { timeZone: zone }) // throws on unknown zones
    return zone
  } catch {
    return undefined
  }
}
