import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const changePassword = vi.fn()
const sessionFetch = vi.fn()
const navigate = vi.fn()

vi.mock('@genesy/api-client/endpoints/farm-logins', () => ({
  farmLoginsChangePassword: (a: unknown) => changePassword(a),
}))
vi.mock('@/app/session', () => ({
  sessionQuery: { queryKey: ['session'], queryFn: () => sessionFetch(), staleTime: 0 },
}))
vi.mock('@tanstack/react-router', () => ({ useNavigate: () => navigate }))

const { SetPasswordScreen } = await import('@/app/SetPasswordScreen')
const { clearTempPassword, holdTempPassword, peekTempPassword } = await import('@/app/tempPassword')
const i18n = (await import('@/i18n')).default

const TEMP = 'k7qxm2pa'
const FARMER = { memberships: [{ id: 'm', role: 'farmer', project_id: 'p', village_id: 'v', revoked_at: null }] }
const failed = (status: number, data: unknown) => ({ response: { status, data } })

function renderScreen() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={queryClient}>
      <SetPasswordScreen />
    </QueryClientProvider>,
  )
}

async function fill(password: string, confirm = password, current?: string) {
  const user = userEvent.setup()
  if (current !== undefined) await user.type(screen.getByTestId('set-password-current'), current)
  await user.type(screen.getByTestId('set-password-new'), password)
  await user.type(screen.getByTestId('set-password-confirm'), confirm)
  await user.click(screen.getByTestId('set-password-submit'))
}

/** Every key and value in both browser stores, as one string to search. */
function browserStorage() {
  const dump: string[] = []
  for (const store of [localStorage, sessionStorage]) {
    for (let i = 0; i < store.length; i++) {
      const key = store.key(i) ?? ''
      dump.push(key, store.getItem(key) ?? '')
    }
  }
  return dump.join('\n')
}

beforeEach(() => {
  changePassword.mockReset()
  sessionFetch.mockReset()
  navigate.mockReset()
  clearTempPassword()
  localStorage.clear()
  sessionStorage.clear()
  // Straight from sign-in: the temporary password is held in memory.
  holdTempPassword(TEMP)
})

/**
 * A farmer's first sign-in uses the temporary password their officer read out.
 * Until they choose their own, the API refuses their survey answers — so the
 * officer who knows the temporary one cannot answer on their behalf.
 */
describe('choosing your own password', () => {
  test('sends the held temporary password as current, then goes home', async () => {
    changePassword.mockResolvedValue(undefined)
    sessionFetch.mockResolvedValue(FARMER)

    renderScreen()
    expect(screen.queryByTestId('set-password-current')).not.toBeInTheDocument()
    await fill('mango-tree-42')

    await waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: '/farm', replace: true }))
    expect(changePassword).toHaveBeenCalledWith({ current_password: TEMP, password: 'mango-tree-42' })
    expect(changePassword.mock.invocationCallOrder[0]).toBeLessThan(
      sessionFetch.mock.invocationCallOrder[0],
    )
  })

  test('the held password leaves shared memory as soon as the screen opens', () => {
    renderScreen()

    expect(peekTempPassword()).toBeNull()
    expect(browserStorage()).not.toContain(TEMP)
  })

  test('two different entries never reach the server', async () => {
    renderScreen()
    await fill('mango-tree-42', 'mango-tree-43')

    expect(await screen.findByTestId('set-password-confirm-error')).toHaveTextContent(
      'The two passwords are not the same.',
    )
    expect(changePassword).not.toHaveBeenCalled()
  })

  // Password rules are the API's. Its field message is written to be read.
  test('the API refusing the password is shown as it says it', async () => {
    changePassword.mockRejectedValue(
      failed(422, {
        message: 'The given data was invalid.',
        errors: { password: ['Password should be at least 8 characters.'] },
      }),
    )

    renderScreen()
    await fill('abc')

    expect(await screen.findByTestId('set-password-error')).toHaveTextContent(
      'Password should be at least 8 characters.',
    )
    expect(navigate).not.toHaveBeenCalled()
  })

  test('a message the app names is shown from the bundle, and nothing navigates', async () => {
    changePassword.mockRejectedValue(failed(409, { message: 'choose a new password first' }))

    renderScreen()
    await fill('mango-tree-42')

    expect(await screen.findByTestId('set-password-error')).toHaveTextContent(
      'Choose a new password first.',
    )
    expect(navigate).not.toHaveBeenCalled()
  })

  test('a lost connection still says something', async () => {
    changePassword.mockRejectedValue(new Error('Network Error'))

    renderScreen()
    await fill('mango-tree-42')

    expect(await screen.findByTestId('set-password-error')).toHaveTextContent(
      i18n.t('setPassword.unexpected'),
    )
  })

  test('the button is disabled while the change is in flight', async () => {
    let release: (v: unknown) => void = () => {}
    changePassword.mockReturnValue(new Promise((r) => (release = r)))
    sessionFetch.mockResolvedValue(FARMER)

    renderScreen()
    await fill('mango-tree-42')

    await waitFor(() => expect(screen.getByTestId('set-password-submit')).toBeDisabled())
    release(undefined)
    await waitFor(() => expect(navigate).toHaveBeenCalled())
  })
})

// After a reload nothing is held, so the farmer types the temporary password.
describe('with no temporary password in memory', () => {
  beforeEach(() => clearTempPassword())

  test('asks for it and sends what was typed', async () => {
    changePassword.mockResolvedValue(undefined)
    sessionFetch.mockResolvedValue(FARMER)

    renderScreen()
    expect(screen.getByLabelText(i18n.t('setPassword.current'))).toBeInTheDocument()
    await fill('mango-tree-42', 'mango-tree-42', 'typed-temp')

    await waitFor(() =>
      expect(changePassword).toHaveBeenCalledWith({
        current_password: 'typed-temp',
        password: 'mango-tree-42',
      }),
    )
  })

  test('an empty current password never reaches the server', async () => {
    renderScreen()
    await fill('mango-tree-42')

    expect(await screen.findByTestId('set-password-error')).toHaveTextContent(
      i18n.t('setPassword.currentRequired'),
    )
    expect(changePassword).not.toHaveBeenCalled()
  })
})
