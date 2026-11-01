import { useQuery } from '@tanstack/react-query'
import { farmRecordsPersonsList } from '@genesy/api-client/endpoints/farm-records'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import type { PeopleSearch, VerificationStatus } from '@/features/officer/peopleSearch'
import { provenanceOf } from '@/features/officer/useOfficerRecords'
import { queryKeys } from '@/lib/queryKeys'
import type { Enums } from '@/lib/enums'

type SourceType = Enums['source_type']
type ConfidenceLevel = Enums['confidence_level']

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
 * People in the officer's villages — spec 5.3. `GET …/farm/persons` (every page) answers only with
 * what this officer may see; the filters are the API's `?verification=` and `?q=` (name, or phone for staff).
 */
export async function fetchPeople(search: PeopleSearch): Promise<PersonRow[]> {
  const workspace = workspaceId()
  const filter = {
    ...(search.verification ? { verification: search.verification } : {}),
    ...(search.q ? { q: search.q } : {}),
  }
  const people = await allPages((params) => farmRecordsPersonsList(workspace, { ...params, ...filter }))

  return people
    .map((p) => ({
      id: p.id,
      given_name: p.given_name,
      family_name: p.family_name,
      phone: p.phone,
      village_id: p.village_id,
      ...provenanceOf(p.provenance),
    }))
    .sort((a, b) => a.family_name.localeCompare(b.family_name) || a.given_name.localeCompare(b.given_name))
}

export function usePeople(search: PeopleSearch) {
  return useQuery({
    // 'mine' rather than a village id: the API decides which villages, so the filters are what
    // distinguish one cached answer from another. Same convention as farms('mine').
    queryKey: queryKeys.people('mine', search),
    queryFn: () => fetchPeople(search),
  })
}
