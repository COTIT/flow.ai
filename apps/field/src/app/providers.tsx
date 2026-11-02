import { QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'

import { queryClient } from '@/app/queryClient'
import { createAppRouter } from '@/app/router'
import { installBugTrail } from '@/lib/bugTrail'
import '@/i18n'

const router = createAppRouter(queryClient)
installBugTrail(router)

export function AppProviders() {
  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  )
}
