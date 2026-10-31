import { useQuery } from '@tanstack/react-query'

import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'

/**
 * The display name behind a provenance `captured_by`.
 *
 * `app_user` reads are self-only, so names come from `app_actor_names`, which
 * returns a name only for actors on records the caller can already see. Zero
 * rows means "not yours to see": the badge omits the name rather than showing
 * a raw id.
 */
export function useActorName(actorId: string | null | undefined): string | undefined {
  const query = useQuery({
    queryKey: queryKeys.actorName(actorId ?? ''),
    enabled: !!actorId,
    staleTime: Infinity,
    queryFn: async () => {
      const { data, error } = await supabase.rpc('app_actor_names', { p_ids: [actorId!] })
      if (error) throw new Error(error.message)
      return data?.[0]?.display_name ?? null
    },
  })
  return query.data ?? undefined
}
