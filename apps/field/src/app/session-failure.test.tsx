import { RouterProvider, createMemoryHistory, createRouter } from '@tanstack/react-router'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { expect, test, vi } from 'vitest'
import { sessionStore } from '@genesy/api-client/session'

// An HTTP answer, so the session query's connection retry does not apply.
const FAILURE = Object.assign(new Error('Request failed with status code 500'), {
  response: { status: 500, data: { message: 'could not connect to the database' } },
})

vi.mock('@/lib/farmMe', () => ({ getFarmMe: () => Promise.reject(FAILURE) }))

const { routeTree } = await import('@/routeTree.gen')
await import('@/i18n')

/**
 * A broken session read must not be mistaken for "no memberships". If it were,
 * resolveLanding would send a perfectly valid officer to /no-access and a
 * transient failure would read as a permissions problem.
 */
test('a failed session read shows an error, not the no-access screen', async () => {
  localStorage.clear()
  sessionStore.getState().signIn({ access_token: 'tok', token_type: 'Bearer', workspace_ids: ['w1'] })
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
  expect(screen.queryByTestId('no-access')).not.toBeInTheDocument()
})
