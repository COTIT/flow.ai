import { sceneKey, type FlowCapability, type SceneKey } from "./capabilities"

/** How often (and when last, ms) the person used each capability on this device. */
export type Uses = Record<string, { n: number; last: number }>

/** Uses before a capability counts as frequent. */
const FREQUENT = 3

/**
 * Deterministic suggestion slots (no AI): 1 frequent · 2 contextual · 3 attention · 4 unfamiliar;
 * free slots take more contextual entries, then the registry order. Same input, same pills.
 */
export function rankPills({
  list,
  scene,
  may,
  uses,
  attention,
  max = 4,
}: {
  list: FlowCapability[]
  scene: SceneKey | undefined
  may: (right: string) => boolean
  uses: Uses
  /** Flow noticed kinds with something open, in Flow noticed order. */
  attention: string[]
  max?: number
}): string[] {
  const here = (c: FlowCapability) =>
    "href" in c.entry && sceneKey(c.entry.href.split("#")[0]!) === scene
  const offered = list.filter(
    (c) => !c.planned && c.rights.every(may) && !here(c)
  )
  const count = (c: FlowCapability) => uses[c.id]?.n ?? 0
  const contextual = offered.filter((c) => scene && c.scenes.includes(scene))
  const frequent = offered
    .filter((c) => count(c) >= FREQUENT)
    .sort(
      (a, b) => count(b) - count(a) || uses[b.id]!.last - uses[a.id]!.last
    )[0]
  const alert = attention
    .map((kind) => offered.find((c) => c.attention === kind))
    .find(Boolean)
  const picked = new Set<FlowCapability>()
  const take = (c?: FlowCapability) => c && picked.add(c)
  take(frequent)
  take(contextual.find((c) => !picked.has(c)))
  if (alert && !picked.has(alert)) take(alert)
  take(offered.find((c) => !picked.has(c) && !count(c)))
  for (const c of [...contextual, ...offered]) take(c)
  return [...picked].slice(0, max).map((c) => c.id)
}

/** "Here's what you can do next" after a completed operation, only what the role may use. */
export const nextActions = (
  list: FlowCapability[],
  operationId: string | undefined,
  may: (right: string) => boolean,
  max = 4
) =>
  operationId
    ? list
        .filter(
          (c) =>
            !c.planned && c.rights.every(may) && c.after?.includes(operationId)
        )
        .slice(0, max)
    : []
