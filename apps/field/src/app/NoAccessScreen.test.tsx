import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const signOut = vi.fn()
const navigate = vi.fn()

vi.mock('@/app/session', () => ({ signOut: (c: unknown) => signOut(c) }))
vi.mock('@tanstack/react-router', () => ({ useNavigate: () => navigate }))

const { NoAccessScreen } = await import('@/app/NoAccessScreen')
await import('@/i18n')

function renderScreen() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={queryClient}>
      <NoAccessScreen />
    </QueryClientProvider>,
  )
}

beforeEach(() => {
  signOut.mockReset()
  navigate.mockReset()
})

describe('NoAccessScreen', () => {
  // Spec 4.4: a real screen with a next step, never a blank page or a crash.
  test('names a next step rather than just refusing', () => {
    renderScreen()
    expect(screen.getByTestId('no-access')).toHaveTextContent(/programme manager/i)
  })

  test('signing out returns to login', async () => {
    signOut.mockResolvedValue(undefined)
    renderScreen()

    await userEvent.click(screen.getByTestId('sign-out'))
    await waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: '/login', replace: true }))
  })

  test('the button reports progress while signing out', async () => {
    let release: () => void = () => {}
    signOut.mockReturnValue(new Promise<void>((r) => (release = r)))
    renderScreen()

    await userEvent.click(screen.getByTestId('sign-out'))
    await waitFor(() => expect(screen.getByTestId('sign-out')).toBeDisabled())

    release()
    await waitFor(() => expect(navigate).toHaveBeenCalled())
  })

  // Without this the button silently does nothing and the user is stranded on
  // a screen whose only action appears broken.
  test('a failed sign out is reported and does not navigate', async () => {
    signOut.mockRejectedValue(new Error('could not reach the auth server'))
    renderScreen()

    await userEvent.click(screen.getByTestId('sign-out'))

    expect(await screen.findByTestId('sign-out-error')).toHaveTextContent(
      'could not reach the auth server',
    )
    expect(navigate).not.toHaveBeenCalled()
    expect(screen.getByTestId('sign-out')).toBeEnabled()
  })
})
