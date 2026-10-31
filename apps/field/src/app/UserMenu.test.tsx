import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, test, vi } from 'vitest'

const useSession = vi.fn()

vi.mock('@/app/session', () => ({ useSession: () => useSession(), signOut: vi.fn() }))
vi.mock('@/app/LanguageSwitch', () => ({ LanguageSwitch: () => <div data-testid="language-switch" /> }))
vi.mock('@/app/tour/TourButton', () => ({ TourButton: () => null }))
vi.mock('@tanstack/react-router', () => ({ useNavigate: () => vi.fn() }))

const { UserMenu } = await import('@/app/UserMenu')
await import('@/i18n')

function renderMenu() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={client}>
      <UserMenu />
    </QueryClientProvider>,
  )
}

describe('UserMenu', () => {
  test('a user without a display name still gets a working trigger, with no blank name line', async () => {
    useSession.mockReturnValue({ data: { appUser: null }, isLoading: false })
    renderMenu()

    await userEvent.click(screen.getByTestId('user-menu-trigger'))

    expect(await screen.findByTestId('user-menu-popup')).toBeInTheDocument()
    expect(screen.queryByTestId('user-menu-name')).not.toBeInTheDocument()
    expect(screen.getByTestId('language-switch')).toBeInTheDocument()
    expect(screen.getByTestId('sign-out')).toBeInTheDocument()
  })

  test('the trigger is labelled for a screen reader before it is ever opened', () => {
    useSession.mockReturnValue({ data: { appUser: { display_name: 'Asha' } }, isLoading: false })
    renderMenu()

    expect(screen.getByTestId('user-menu-trigger')).toHaveAccessibleName()
    expect(screen.queryByTestId('user-menu-popup')).not.toBeInTheDocument()
  })
})
