import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const updateFarmMe = vi.fn()
const useSession = vi.fn()
/** An axios error: the API answered 403. */
const REFUSED = Object.assign(new Error('Request failed with status code 403'), {
  response: { status: 403, data: { message: 'This action is unauthorized.' } },
})
/** The route the shell is currently on. Mutated by the navigation tests. */
let pathname = '/officer/register'

vi.mock('@/lib/farmMe', () => ({ updateFarmMe: (v: unknown) => updateFarmMe(v) }))
vi.mock('@/app/session', () => ({ useSession: () => useSession() }))
vi.mock('@tanstack/react-router', () => ({
  useRouterState: ({ select }: { select: (s: unknown) => unknown }) =>
    select({ location: { pathname } }),
}))

const { LanguageSwitch } = await import('@/app/LanguageSwitch')
const i18n = (await import('@/i18n')).default

function renderSwitch(className = '') {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const result = render(
    <QueryClientProvider client={queryClient}>
      <LanguageSwitch className={className} />
    </QueryClientProvider>,
  )
  return {
    // Re-renders through the same provider, so the mutation state survives —
    // which is the point of the navigation tests.
    rerender: () =>
      result.rerender(
        <QueryClientProvider client={queryClient}>
          <LanguageSwitch className={className} />
        </QueryClientProvider>,
      ),
  }
}

async function chooseLanguage(label: string) {
  await userEvent.click(screen.getByTestId('language-switch'))
  await userEvent.click(await screen.findByRole('option', { name: label }))
}

const signedIn = (locale: string) => ({
  data: { appUser: { id: 'u1', person_id: null, display_name: 'Salima', locale }, memberships: [] },
  isLoading: false,
})

beforeEach(async () => {
  updateFarmMe.mockReset()
  useSession.mockReset()
  pathname = '/officer/register'
  await i18n.changeLanguage('en')
})

describe('LanguageSwitch loading and signed-out states', () => {
  test('accepts shell layout classes without changing the control semantics', () => {
    useSession.mockReturnValue({ data: undefined, isLoading: true })
    renderSwitch('w-full sm:w-auto')

    expect(screen.getByTestId('language-switch')).toHaveClass('w-full', 'sm:w-auto')
    expect(screen.getByLabelText(/language/i)).toBeInTheDocument()
  })

  test('while the session resolves it still works, in memory only', async () => {
    useSession.mockReturnValue({ data: undefined, isLoading: true })
    renderSwitch()

    const select = screen.getByTestId('language-switch')
    expect(select).toBeEnabled()

    await chooseLanguage('Kiswahili')
    await waitFor(() => expect(i18n.resolvedLanguage).toBe('sw'))
    // Nothing to persist to yet, so nothing is attempted.
    expect(updateFarmMe).not.toHaveBeenCalled()
  })

  test('signed out, the choice is not persisted', async () => {
    useSession.mockReturnValue({ data: null, isLoading: false })
    renderSwitch()

    await chooseLanguage('Kiswahili')
    await waitFor(() => expect(i18n.resolvedLanguage).toBe('sw'))
    expect(updateFarmMe).not.toHaveBeenCalled()
  })
})

describe('LanguageSwitch persistence', () => {
  test('a change is written to the account locale', async () => {
    useSession.mockReturnValue(signedIn('en'))
    updateFarmMe.mockResolvedValue(undefined)
    renderSwitch()

    await chooseLanguage('Kiswahili')
    await waitFor(() => expect(updateFarmMe).toHaveBeenCalledWith({ locale: 'sw' }))
  })

  test('the control is disabled while the write is in flight', async () => {
    useSession.mockReturnValue(signedIn('en'))
    let release: (v: unknown) => void = () => {}
    updateFarmMe.mockReturnValue(new Promise((r) => (release = r)))
    renderSwitch()

    await chooseLanguage('Kiswahili')
    await waitFor(() => expect(screen.getByTestId('language-switch')).toBeDisabled())

    release(undefined)
    await waitFor(() => expect(screen.getByTestId('language-switch')).toBeEnabled())
  })

  // The gap this pass exists to close: the write could fail and say nothing.
  // The language had already changed on screen, so the user believed it saved.
  test('a refused write is reported rather than failing silently', async () => {
    useSession.mockReturnValue(signedIn('en'))
    updateFarmMe.mockRejectedValue(REFUSED)
    renderSwitch()

    await chooseLanguage('Kiswahili')

    expect(await screen.findByTestId('language-error')).toHaveTextContent(i18n.t('language.notSaved'))
  })

  test('a rejected write is reported too', async () => {
    useSession.mockReturnValue(signedIn('en'))
    updateFarmMe.mockRejectedValue(new Error('offline'))
    renderSwitch()

    await chooseLanguage('Kiswahili')
    expect(await screen.findByTestId('language-error')).toHaveTextContent('offline')
  })

  test('the error clears on a later successful write', async () => {
    useSession.mockReturnValue(signedIn('en'))
    updateFarmMe.mockRejectedValueOnce(new Error('offline'))
    renderSwitch()

    await chooseLanguage('Kiswahili')
    await screen.findByTestId('language-error')

    updateFarmMe.mockResolvedValueOnce(undefined)
    await chooseLanguage('Kiingereza')
    await waitFor(() => expect(screen.queryByTestId('language-error')).not.toBeInTheDocument())
  })
})

describe('LanguageSwitch applies the stored locale', () => {
  test('the stored locale is adopted once the session arrives', async () => {
    useSession.mockReturnValue(signedIn('sw'))
    renderSwitch()
    await waitFor(() => expect(i18n.resolvedLanguage).toBe('sw'))
  })

  test('an unsupported stored locale is ignored rather than applied blindly', async () => {
    useSession.mockReturnValue(signedIn('fr'))
    renderSwitch()
    await waitFor(() => expect(i18n.resolvedLanguage).toBe('en'))
  })
})

/**
 * QA #25 and #24. The BEHAVIOUR here is right and worth keeping: the switch
 * tells the truth — "changed for now, but could not be saved" — rather than
 * silently pretending it saved. Only the message and its lifetime were wrong.
 */
describe('what the failure says, and how long it says it', () => {
  test('a JS exception is not what the officer reads', async () => {
    useSession.mockReturnValue(signedIn('en'))
    updateFarmMe.mockRejectedValue(Object.assign(new Error('Network Error'), { isAxiosError: true, code: 'ERR_NETWORK' }))
    renderSwitch()

    await chooseLanguage('Kiswahili')

    // The switch just changed the screen to Swahili, so the banner reads in
    // Swahili: the network message and the honest half both come from the
    // bundle in the language now showing.
    const banner = await screen.findByTestId('language-error')
    expect(banner).toHaveTextContent(i18n.t('error.network'))
    expect(banner).not.toHaveTextContent('Network Error')
    // The honest half stays: it changed on screen and was not stored.
    expect(banner).toHaveTextContent(i18n.t('language.notSaved'))
  })

  /**
   * QA #24. The banner described an event that was over, on a screen with
   * nothing to do with it — it survived the route change and two more after
   * that.
   */
  test('the banner does not follow the user to the next screen', async () => {
    useSession.mockReturnValue(signedIn('en'))
    updateFarmMe.mockRejectedValue(new Error('offline'))
    const { rerender } = renderSwitch()

    await chooseLanguage('Kiswahili')
    await screen.findByTestId('language-error')

    pathname = '/officer/people'
    rerender()

    await waitFor(() =>
      expect(screen.queryByTestId('language-error')).not.toBeInTheDocument(),
    )
  })

  test('but it survives a re-render on the same screen', async () => {
    useSession.mockReturnValue(signedIn('en'))
    updateFarmMe.mockRejectedValue(new Error('offline'))
    const { rerender } = renderSwitch()

    await chooseLanguage('Kiswahili')
    await screen.findByTestId('language-error')

    rerender()

    expect(screen.getByTestId('language-error')).toBeInTheDocument()
  })
})
