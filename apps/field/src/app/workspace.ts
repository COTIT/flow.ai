import { useSession as useVerzoSession } from '@verzo/api-client/auth'
import { sessionStore } from '@verzo/api-client/session'

/**
 * The Verzo workspace the farm screens work in (RUAHA's project): the one chosen at sign-in,
 * from the shared session store. Every farm route is `/v1/workspace/{workspace}/farm/...`.
 */
export function useWorkspaceId(): string | null {
  return useVerzoSession((s) => s.workspaceId)
}

/** Same, outside React (query functions, route loaders). */
export function workspaceId(): string {
  const id = sessionStore.getState().workspaceId
  if (!id) throw new Error('No workspace selected: sign in again.')
  return id
}
