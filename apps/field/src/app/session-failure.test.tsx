import { RouterProvider, createMemoryHistory, createRouter } from '@tanstack/react-router'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { expect, test, vi } from 'vitest'

const FAILURE = 'could not connect to the database'

vi.mock('@/lib/supabase', () => ({
  supabase: {
    auth: {
      getSession: async () => ({ data: { session: { user: { id: 'u1', email: null } } } }),
    },
    from: (table: string) =>
      table === 'app_user'
        ? { select: () => ({ eq: () => ({ maybeSingle: async () => ({ data: null, error: null }) }) }) }
        : { select: () => ({ eq: () => ({ is: async () => ({ data: null, error: { message: FAILURE } }) }) }) },
  },
}))

const { routeTree } = await import('@/routeTree.gen')
await import('@/i18n')

/**
 * A broken session read must not be mistaken for "no memberships". If it were,
 * resolveLanding would send a perfectly valid officer to /no-access and a
 * transient failure would read as a permissions problem.
 */
test('a failed session read shows an error, not the no-access screen', async () => {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const router = createRouter({
    routeTree,
    context: { queryClient },
    history: createMemoryHistory({ initialEntries: ['/ops'] }),
  })

  render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router as never} />
    </QueryClientProvider>,
  )

  expect(await screen.findByTestId('error-state')).toBeInTheDocument()
  // Surfaced verbatim, per the error contract.
  expect(screen.getByText(FAILURE)).toBeInTheDocument()
  expect(screen.queryByTestId('no-access')).not.toBeInTheDocument()
})
