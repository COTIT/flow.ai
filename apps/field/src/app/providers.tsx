import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'

import { createAppRouter } from '@/app/router'
import { installBugTrail } from '@/lib/bugTrail'
import '@/i18n'

// Zero rows is a legitimate answer, not an error: RLS returning nothing means
// "you may not see this". Never retry a query into an empty result.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: false,
      refetchOnWindowFocus: false,
      staleTime: 30_000,
    },
  },
})

const router = createAppRouter(queryClient)
installBugTrail(router)

export function AppProviders() {
  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  )
}
