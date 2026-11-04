import { useQuery } from '@tanstack/react-query'
import { farmVillagesList } from '@genesy/api-client/endpoints/farm-villages'

import { fetchSession } from '@/app/session'
import { queryKeys } from '@/lib/queryKeys'

export interface ScopeNames {
  villages: Record<string, string>
  /** Genesy workspaces (RUAHA projects). */
  projects: Record<string, string>
}

/**
 * Village and workspace names, for labelling a membership's scope. Workspace
 * names come with the caller's roles; villages are read per workspace, since
 * ops and admin cover villages their role row does not name.
 */
export async function fetchScopeNames(): Promise<ScopeNames> {
  const session = await fetchSession()
  const projects: Record<string, string> = {}
  for (const m of session?.me.memberships ?? []) projects[m.workspace_id] = m.display_name

  // A failed read throws: labelling a membership "no village" because a query
  // broke would misdescribe someone's access.
  // shortcut: first 100 villages per workspace (API max page size); page further when a workspace has more.
  const pages = await Promise.all(
    Object.keys(projects).map((workspace) => farmVillagesList(workspace, { page_size: 100 })),
  )
  const villages: Record<string, string> = {}
  for (const page of pages) for (const v of page.data) villages[v.id] = v.name
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
