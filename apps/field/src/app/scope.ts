import { useQuery } from '@tanstack/react-query'

import { supabase } from '@/lib/supabase'
import { queryKeys } from '@/lib/queryKeys'

export interface ScopeNames {
  villages: Record<string, string>
  projects: Record<string, string>
}

/**
 * Village and project names, for labelling a membership's scope.
 *
 * Both tables are readable by any signed-in user — village names are not
 * personal data and every surface needs them to render — so this is one cheap
 * lookup rather than a join through a restricted table.
 */
export async function fetchScopeNames(): Promise<ScopeNames> {
  const [villageResult, projectResult] = await Promise.all([
    supabase.from('village').select('id, name'),
    supabase.from('project').select('id, name'),
  ])

  // A failed read is not an empty set: labelling a membership "no village"
  // because a query broke would misdescribe someone's access.
  if (villageResult.error) throw new Error(villageResult.error.message)
  if (projectResult.error) throw new Error(projectResult.error.message)

  const villages: Record<string, string> = {}
  for (const v of villageResult.data ?? []) villages[v.id] = v.name
  const projects: Record<string, string> = {}
  for (const p of projectResult.data ?? []) projects[p.id] = p.name

  return { villages, projects }
}

export function useScopeNames() {
  return useQuery({
    queryKey: queryKeys.villages(),
    queryFn: fetchScopeNames,
    // Config data, effectively static for a session.
    staleTime: 5 * 60_000,
  })
}
