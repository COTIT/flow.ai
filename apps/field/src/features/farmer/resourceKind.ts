/**
 * The two kinds of resource in the catalogue. Held in the URL (`?kind=loan`)
 * so a link or a reload lands on the same list; equipment is the default and
 * is left out of the URL. Anything unrecognised degrades to equipment.
 */
export type ResourceKind = 'equipment' | 'loan'

export function validateResourceSearch(search: Record<string, unknown>): { kind?: 'loan' } {
  return search.kind === 'loan' ? { kind: 'loan' } : {}
}

export const kindOf = (search: { kind?: string }): ResourceKind =>
  search.kind === 'loan' ? 'loan' : 'equipment'
