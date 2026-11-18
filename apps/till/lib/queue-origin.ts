// No imports: the till's unit tests run this file in Node directly.

/** Where a queued sale was rung up: workspace, register and till user (null: not recorded). */
export type Origin = {
  workspace: string
  registerId: string | null
  tillUserId: string | null
}

/**
 * The queued sale's origin against the live session. Only `here` may be posted: the server books
 * a sale under the session's register and till user, whatever the body says.
 */
export type Where = "here" | "otherUser" | "otherRegister" | "unknown"

export function whereFrom(
  item: Origin,
  now: { workspace?: string; registerId?: string; tillUserId?: string }
): Where {
  if (item.registerId == null || item.tillUserId == null) return "unknown"
  if (item.workspace !== now.workspace || item.registerId !== now.registerId)
    return "otherRegister"
  return item.tillUserId === now.tillUserId ? "here" : "otherUser"
}

/** Stored queue v0 kept no origin: its sales become `unknown`, looked up but never posted. */
export function migrateQueue(
  persisted: unknown,
  version: number
): { items: unknown[] } {
  const items = (persisted as { items?: unknown[] } | undefined)?.items ?? []
  if (version >= 1) return { items }
  return {
    items: items.map((i) => ({
      ...(i as object),
      registerId: null,
      tillUserId: null,
    })),
  }
}
