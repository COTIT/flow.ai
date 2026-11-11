import "server-only"
import { voiceMessages } from "@genesy/i18n/voice"
import { getOperation, isToolAllowed } from "./catalog"
import { CAPABILITIES, rotated, type Available } from "@genesy/voice/lib/capabilities"

const messages = voiceMessages
export const voiceText = (language?: string | null) =>
  messages[(language ?? "en") as keyof typeof messages]?.voice ?? messages.en.voice

const runnable = (id: string) => {
  const op = getOperation(id)
  return op?.disposition === "execute" && op.availability !== "unavailable"
    ? op
    : undefined
}

/** Curated entries whose operations the voice catalog can run, with their rights (for chips). */
export function availableCapabilities(): Available[] {
  return CAPABILITIES.flatMap(({ ops, ...entry }) => {
    const tools = ops.map(runnable)
    if (tools.some((tool) => !tool || tool.authorization.roleAnyOf.length))
      return []
    const rights = (key: "allOf" | "anyOf") => [
      ...new Set(tools.flatMap((tool) => tool!.authorization[key])),
    ]
    return [
      { ...entry, rights: { allOf: rights("allOf"), anyOf: rights("anyOf") } },
    ]
  })
}

/** The "What can you do?" answer: at most seven plain items the member may use, grouped. */
export function describeCapabilities(
  permissions: string[],
  language?: string | null
) {
  const text = voiceText(language)
  const labels = text.capabilities as Record<string, { label: string }>
  const groups = text.capabilityGroups as Record<string, string>
  const allowed = new Set(
    CAPABILITIES.filter((c) =>
      c.ops.every((id) => {
        const tool = runnable(id)
        return tool && isToolAllowed(tool, permissions)
      })
    ).map((c) => c.id)
  )
  const items = rotated(availableCapabilities(), 0)
    .filter((c) => allowed.has(c.id))
    .slice(0, 7)
  return {
    groups: [...new Set(items.map((c) => c.module))].map((module) => ({
      group: groups[module],
      items: items
        .filter((c) => c.module === module)
        .map((c) => labels[c.id]!.label),
    })),
    pages: text.pagesHint,
  }
}
