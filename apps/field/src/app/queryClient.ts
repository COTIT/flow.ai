import { QueryClient } from '@tanstack/react-query'

// Zero rows is a legitimate answer, not an error: RLS returning nothing means
// "you may not see this". Never retry a query into an empty result.
// networkMode 'always': offline, a request fails fast into "No network" with a retry instead of
// pausing (an endless "Loading…", or an empty list).
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: false,
      refetchOnWindowFocus: false,
      staleTime: 30_000,
      networkMode: 'always',
    },
    mutations: { networkMode: 'always' },
  },
})
