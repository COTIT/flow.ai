/** Office detail pages a voice write can point at; the param is always the record's uuid. */
const PAGES = new Set(["orders", "invoices", "products", "lots"])
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

export type RecordLink = { label: string | null; href: string }

const record = (data: unknown): Record<string, unknown> | undefined => {
  if (!data || typeof data !== "object" || Array.isArray(data)) return
  const value = data as Record<string, unknown>
  return typeof value.id === "string" ? value : record(value.data)
}

/** The record a completed write created or changed, as an office link. The label is the
 * record's own number or name; null when the response does not carry one. */
export function recordLinks(
  action: { write: boolean; path: string },
  data: unknown
): RecordLink[] {
  if (!action.write) return []
  const [, , , resource, pathId] = action.path.split(/[/?]/)
  if (!resource || !PAGES.has(resource)) return []
  const body = record(data)
  const id = pathId && UUID.test(pathId) ? pathId : body?.id
  if (typeof id !== "string" || !UUID.test(id)) return []
  const own = body?.id === id ? body : undefined
  const label = [own?.number, own?.name, own?.title, own?.reference].find(
    (item): item is string => typeof item === "string" && item.trim() !== ""
  )
  return [{ label: label ?? null, href: `/${resource}/${id.toLowerCase()}` }]
}
