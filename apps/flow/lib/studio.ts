import { useStore } from "zustand"
import { createStore } from "zustand/vanilla"


/** "vN" for a revision on the loaded page (newest first, append-only), else "—". */
export function revisionLabel(
  page:
    | {
        data: { revision_id: string }[]
        meta: { total: number; from?: number | null }
      }
    | undefined,
  id: string | null | undefined
): string {
  const index = page && id ? page.data.findIndex((r) => r.revision_id === id) : -1
  if (!page || index < 0) return "—"
  return `v${page.meta.total - (page.meta.from ?? 1) + 1 - index}`
}

export type LinkState = "none" | "active" | "hidden" | "expired" | "revoked" | "ended"

/**
 * What the card shows for the draft's newest link. Active needs the token from this tab's memory;
 * without it (reload, another tab) the link exists but cannot be opened here ("hidden").
 */
export function linkView<L extends { status: string }>(
  links: L[],
  token: string | null
): { state: LinkState; link: L | null } {
  const link = links[0] ?? null
  if (!link) return { state: "none", link }
  if (link.status === "active") return { state: token ? "active" : "hidden", link }
  return { state: link.status as LinkState, link }
}

export type Brief = {
  areas: string[]
  keepStructure: boolean
  direction: "editorial" | "product"
}

const list = (items: string[]) =>
  items.length < 2
    ? (items[0] ?? "")
    : `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`

/** The one plain request the brief sends; the assistant's edit tools and review do the rest. */
export function briefText({ areas, keepStructure, direction }: Brief): string {
  return [
    `Redesign my storefront: change the ${list(areas.map((a) => a.toLowerCase()))}`,
    keepStructure
      ? "keep the existing product structure"
      : "the product structure may change",
    `use ${direction === "editorial" ? "an editorial" : "a product-focused"} layout.`,
  ].join("; ")
}

/**
 * Studio state of this tab only, in memory (no persist): preview tokens, the redesign request
 * waiting for its draft, and a dismissed resume prompt. Preview tokens are shown once by the API;
 * they never go to storage, cookies, URLs, query caches or logs. Reload forgets all of it.
 */
type StudioState = {
  token: { revisionId: string; token: string } | null
  /** A brief or "another change" was sent while `from` was the draft; its successor gets a link. */
  awaiting: { from: string | null } | null
  dismissed: boolean
}

export const studioStore = createStore<StudioState>()(() => ({
  token: null,
  awaiting: null,
  dismissed: false,
}))

export const useStudio = <T,>(pick: (state: StudioState) => T) =>
  useStore(studioStore, pick)

export function rememberToken(revisionId: string, token: string) {
  studioStore.setState({ token: { revisionId, token } })
}

export function tokenFor(revisionId: string | null | undefined): string | null {
  const held = studioStore.getState().token
  return revisionId && held?.revisionId === revisionId ? held.token : null
}

export function forgetTokens() {
  studioStore.setState({ token: null })
}
