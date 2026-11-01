import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const signOut = vi.fn()
const navigate = vi.fn()

vi.mock('@/app/session', () => ({ signOut }))
vi.mock('@tanstack/react-router', () => ({ useNavigate: () => navigate }))

const { SignOutButton } = await import('@/app/SignOutButton')
await import('@/i18n')

function renderButton() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={client}>
      <SignOutButton />
    </QueryClientProvider>,
  )
}

beforeEach(() => {
  signOut.mockReset()
  navigate.mockReset()
  signOut.mockResolvedValue(undefined)
  navigate.mockResolvedValue(undefined)
})

describe('SignOutButton', () => {
  test('renders idle and disabled states while signing out', async () => {
    let resolve!: () => void
    signOut.mockReturnValue(new Promise<void>((r) => { resolve = r }))
    const user = userEvent.setup()
    renderButton()

    const button = screen.getByTestId('sign-out')
    expect(button).toHaveTextContent('Sign out')
    await user.click(button)
    expect(button).toBeDisabled()
    expect(button).toHaveTextContent('Signing out')

    resolve()
  })

  test('clears the session and navigates to login after success', async () => {
    const user = userEvent.setup()
    renderButton()

    await user.click(screen.getByTestId('sign-out'))
    expect(signOut).toHaveBeenCalledTimes(1)
    expect(navigate).toHaveBeenCalledWith({ to: '/login', replace: true })
    expect(screen.queryByTestId('sign-out-error')).not.toBeInTheDocument()
  })

  test('shows a readable error and restores the control after failure', async () => {
    signOut.mockRejectedValue(new Error('Session could not be cleared'))
    const user = userEvent.setup()
    renderButton()

    await user.click(screen.getByTestId('sign-out'))
    expect(screen.getByTestId('sign-out-error')).toHaveTextContent('Session could not be cleared')
    expect(screen.getByTestId('sign-out')).not.toBeDisabled()
    expect(navigate).not.toHaveBeenCalled()
  })
})
