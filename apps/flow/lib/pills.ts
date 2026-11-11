/** A suggestion in the dock: a capability id (stable key) and what it does. */
export type Pill = { id: string; label: string; run: () => void }

/** shortcut: no suggestions until the capability registry and ranking land (PR F3). */
export function usePills(): Pill[] {
  return []
}
