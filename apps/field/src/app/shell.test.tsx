import { RouterProvider, createMemoryHistory, createRouter } from '@tanstack/react-router'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, within } from '@testing-library/react'
import { expect, test } from 'vitest'

import { routeTree } from '@/routeTree.gen'
import '@/i18n'

function renderAt(path: string) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const router = createRouter({
    routeTree,
    context: { queryClient },
    history: createMemoryHistory({ initialEntries: [path] }),
  })

  render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router as never} />
    </QueryClientProvider>,
  )
  return router
}

// The text wordmark became the supplied lockup plus `360` after a hairline
// divider. Ruaha 360 is a programme surface, not a second brand, so the two
// halves are deliberately separate elements — and the brand half has to keep an
// accessible name, or the header would read as a decoration to a screen reader.
test('the shell mounts and renders the brand lockup', async () => {
  renderAt('/login')
  // Scoped to the header: the login card carries a lockup of its own, at 34px.
  const header = await screen.findByRole('banner')
  expect(within(header).getByAltText('Ruaha Energy')).toBeInTheDocument()
  expect(within(header).getByText('360')).toBeInTheDocument()
})

test('an unauthenticated visit to a guarded surface lands on /login', async () => {
  const router = renderAt('/ops/tower')

  // Route guards are UX: with no session there is nothing to scope a Tower to,
  // so the guard sends the visitor to sign in rather than rendering an empty
  // Control Tower.
  expect(await screen.findByTestId('login-submit')).toBeInTheDocument()
  expect(router.state.location.pathname).toBe('/login')
})

test('the demo banner renders when VITE_DATA_MODE is demo', async () => {
  renderAt('/login')
  expect(await screen.findByTestId('demo-banner')).toBeInTheDocument()
})

test('signed out, the shell offers no surface nav and no sign out', async () => {
  renderAt('/login')
  await screen.findByTestId('login-submit')
  expect(screen.queryByTestId('sign-out')).not.toBeInTheDocument()
  expect(screen.queryByText('Control Tower')).not.toBeInTheDocument()
})
