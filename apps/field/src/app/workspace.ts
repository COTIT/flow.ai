import { useSession as useGenesySession } from '@genesy/api-client/auth'
import { sessionStore } from '@genesy/api-client/session'

/**
 * The Genesy workspace the farm screens work in (RUAHA's project): the one chosen at sign-in,
 * from the shared session store. Every farm route is `/v1/workspace/{workspace}/farm/...`.
 */
export function useWorkspaceId(): string | null {
  return useGenesySession((s) => s.workspaceId)
}

/** Same, outside React (query functions, route loaders). */
export function workspaceId(): string {
  const id = sessionStore.getState().workspaceId
  if (!id) throw new Error('No workspace selected: sign in again.')
  return id
}
