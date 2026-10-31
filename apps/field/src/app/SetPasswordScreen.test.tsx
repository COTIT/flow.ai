import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const updateUser = vi.fn()
const rpc = vi.fn()
const sessionFetch = vi.fn()
const navigate = vi.fn()

vi.mock('@/lib/supabase', () => ({
  supabase: {
    auth: { updateUser: (a: unknown) => updateUser(a) },
    rpc: (...a: unknown[]) => rpc(...a),
  },
}))
vi.mock('@/app/session', () => ({
  sessionQuery: { queryKey: ['session'], queryFn: () => sessionFetch(), staleTime: 0 },
}))
vi.mock('@tanstack/react-router', () => ({ useNavigate: () => navigate }))

const { SetPasswordScreen } = await import('@/app/SetPasswordScreen')
await import('@/i18n')

function renderScreen() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={queryClient}>
      <SetPasswordScreen />
    </QueryClientProvider>,
  )
}

const FARMER = { memberships: [{ id: 'm', role: 'farmer', project_id: 'p', village_id: 'v', revoked_at: null }] }

beforeEach(() => {
  updateUser.mockReset()
  rpc.mockReset()
  sessionFetch.mockReset()
  navigate.mockReset()
})

async function fill(password: string, confirm = password) {
  const user = userEvent.setup()
  await user.type(screen.getByTestId('set-password-new'), password)
  await user.type(screen.getByTestId('set-password-confirm'), confirm)
  await user.click(screen.getByTestId('set-password-submit'))
}

/**
 * A farmer's first sign-in uses the temporary password their officer read out.
 * Until they choose their own, the database refuses their survey answers — so
 * the officer who knows the temporary one cannot answer on their behalf.
 */
describe('choosing your own password', () => {
  test('saves the password, then tells the database, then goes home', async () => {
    updateUser.mockResolvedValue({ error: null })
    rpc.mockResolvedValue({ error: null })
    sessionFetch.mockResolvedValue(FARMER)

    renderScreen()
    await fill('mango-tree-42')

    await waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: '/farm', replace: true }))
    expect(updateUser).toHaveBeenCalledWith({ password: 'mango-tree-42' })
    expect(rpc).toHaveBeenCalledWith('app_password_changed')
    expect(updateUser.mock.invocationCallOrder[0]).toBeLessThan(rpc.mock.invocationCallOrder[0])
  })

  test('two different entries never reach the server', async () => {
    renderScreen()
    await fill('mango-tree-42', 'mango-tree-43')

    expect(await screen.findByTestId('set-password-confirm-error')).toHaveTextContent(
      'The two passwords are not the same.',
    )
    expect(updateUser).not.toHaveBeenCalled()
  })

  // Password rules are the auth server's. Its message is written to be read.
  test('the auth server refusing the password is shown as it says it', async () => {
    updateUser.mockResolvedValue({ error: { message: 'Password should be at least 6 characters.' } })

    renderScreen()
    await fill('abc')

    expect(await screen.findByTestId('set-password-error')).toHaveTextContent(
      'Password should be at least 6 characters.',
    )
    expect(rpc).not.toHaveBeenCalled()
  })

  test('the database refusing the change is shown too, and nothing navigates', async () => {
    updateUser.mockResolvedValue({ error: null })
    rpc.mockResolvedValue({ error: { message: 'choose a new password first' } })

    renderScreen()
    await fill('mango-tree-42')

    expect(await screen.findByTestId('set-password-error')).toHaveTextContent(
      'Choose a new password first.',
    )
    expect(navigate).not.toHaveBeenCalled()
  })
})
