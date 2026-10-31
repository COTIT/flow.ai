import { useQuery } from '@tanstack/react-query'

import type { PeopleSearch, VerificationStatus } from '@/features/officer/peopleSearch'
import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'
import type { Database } from '@/lib/db.types'

type SourceType = Database['public']['Enums']['source_type']
type ConfidenceLevel = Database['public']['Enums']['confidence_level']

export interface PersonRow {
  id: string
  given_name: string
  family_name: string
  phone: string | null
  village_id: string
  source: SourceType
  verification: VerificationStatus
  confidence: ConfidenceLevel | null
  captured_at: string
}

/**
 * PostgREST `or` takes a comma-separated filter list, so a comma or a
 * parenthesis in the search term would be read as filter syntax rather than as
 * text. Stripping them keeps a name search a name search.
 */
function escapeForOr(term: string): string {
  return term.replace(/[,()]/g, ' ').trim()
}

/**
 * People in the officer's villages — spec 5.3.
 *
 * No village filter is applied or needed: `person_read_staff` scopes to
 * `app_villages()`, so "people I may see" is the whole query. Filters are
 * applied to the QUERY rather than to rendered rows, so a filtered view is the
 * database's answer and not a slice of a page.
 */
export async function fetchPeople(search: PeopleSearch): Promise<PersonRow[]> {
  let q = supabase
    .from('person')
    .select(
      'id, given_name, family_name, phone, village_id, source, verification, confidence, captured_at',
    )
    .is('deleted_at', null)

  if (search.verification) q = q.eq('verification', search.verification)

  if (search.q) {
    const term = escapeForOr(search.q)
    // Matches either name part, so "Neema" and "Mwakalinga" both find her.
    if (term) q = q.or(`given_name.ilike.%${term}%,family_name.ilike.%${term}%`)
  }

  const { data, error } = await q.order('family_name').order('given_name')
  if (error) throw new Error(error.message)
  return (data ?? []) as PersonRow[]
}

export function usePeople(search: PeopleSearch) {
  return useQuery({
    // 'mine' rather than a village id: an officer may hold several villages and
    // RLS already decides which, so the filters are what distinguish one cached
    // answer from another. Same convention as farms('mine').
    queryKey: queryKeys.people('mine', search),
    queryFn: () => fetchPeople(search),
  })
}
