import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { StrictMode, type ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const rpc = vi.fn()

vi.mock('@/lib/supabase', () => ({ supabase: { rpc: (...args: unknown[]) => rpc(...args) } }))

const { FarmerLoginCard } = await import('@/features/officer/FarmerLoginCard')
await import('@/i18n')

const PERSON = '11111111-1111-4111-8111-111111111111'
const ISSUED = { phone: '+255712000111', temp_password: 'K7QXM2PA', kind: 'initial' }

let queryClient: QueryClient

function renderCard(ui: ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>)
}

const issueCalls = () => rpc.mock.calls.filter(([name]) => name === 'app_farmer_login_issue')

beforeEach(() => {
  rpc.mockReset()
  queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
})

describe('FarmerLoginCard, created by hand', () => {
  test('offers Create app login, and issues nothing until pressed', () => {
    renderCard(<FarmerLoginCard personId={PERSON} />)

    expect(screen.getByTestId('farmer-login-card')).toBeInTheDocument()
    expect(screen.getByTestId('farmer-login-issue')).toHaveTextContent('Create app login')
    expect(screen.queryByTestId('farmer-login-reset')).not.toBeInTheDocument()
    expect(rpc).not.toHaveBeenCalled()
  })

  test('shows the phone and the temporary password once, with the warning', async () => {
    rpc.mockResolvedValue({ data: ISSUED, error: null })
    renderCard(<FarmerLoginCard personId={PERSON} />)

    await userEvent.click(screen.getByTestId('farmer-login-issue'))

    expect(await screen.findByTestId('farmer-login-password')).toHaveTextContent('K7QXM2PA')
    expect(screen.getByTestId('farmer-login-phone')).toHaveTextContent('+255712000111')
    expect(screen.getByTestId('farmer-login-card')).toHaveTextContent('Shown once.')
    expect(rpc).toHaveBeenCalledWith('app_farmer_login_issue', { p_person_id: PERSON })
    // No second action while the credential is on screen.
    expect(screen.queryByTestId('farmer-login-issue')).not.toBeInTheDocument()
  })

  test('the button says so while the login is being created', async () => {
    rpc.mockReturnValue(new Promise(() => {}))
    renderCard(<FarmerLoginCard personId={PERSON} />)

    await userEvent.click(screen.getByTestId('farmer-login-issue'))

    expect(screen.getByTestId('farmer-login-issue')).toBeDisabled()
    expect(screen.getByTestId('farmer-login-issue')).toHaveTextContent('Creating login…')
  })

  // The login now exists. Another press of "Create" would reset the password
  // the farmer was just given, so the action becomes Reset, behind a confirm.
  test('Done clears the password; the next action is a confirmed reset', async () => {
    rpc.mockResolvedValue({ data: ISSUED, error: null })
    renderCard(<FarmerLoginCard personId={PERSON} />)

    await userEvent.click(screen.getByTestId('farmer-login-issue'))
    await userEvent.click(await screen.findByTestId('farmer-login-done'))

    expect(screen.queryByTestId('farmer-login-password')).not.toBeInTheDocument()
    expect(screen.queryByText('K7QXM2PA')).not.toBeInTheDocument()
    expect(screen.getByTestId('farmer-login-reset')).toBeInTheDocument()
    expect(screen.queryByTestId('farmer-login-issue')).not.toBeInTheDocument()
  })

  // business-rules §9: the refusal as the database wrote it.
  test('a refusal is shown verbatim, with a retry that issues again', async () => {
    rpc
      .mockResolvedValueOnce({ data: null, error: { message: 'add a phone number before issuing a login' } })
      .mockResolvedValueOnce({ data: ISSUED, error: null })
    renderCard(<FarmerLoginCard personId={PERSON} />)

    await userEvent.click(screen.getByTestId('farmer-login-issue'))

    const failure = await screen.findByTestId('farmer-login-error')
    expect(failure).toHaveTextContent('The farmer is registered, but the app login could not be created.')
    expect(failure).toHaveTextContent('Add a phone number before issuing a login.')
    expect(screen.getByRole('alert')).toBeInTheDocument()

    await userEvent.click(screen.getByTestId('farmer-login-retry'))
    expect(await screen.findByTestId('farmer-login-password')).toHaveTextContent('K7QXM2PA')
    expect(issueCalls()).toHaveLength(2)
  })
})

describe('FarmerLoginCard, issued on registration', () => {
  // StrictMode runs every effect twice in development. Two calls would create
  // the login and then immediately reset it, invalidating the password shown.
  test('issues exactly once on mount, even under StrictMode', async () => {
    rpc.mockResolvedValue({ data: ISSUED, error: null })
    renderCard(
      <StrictMode>
        <FarmerLoginCard personId={PERSON} autoIssue />
      </StrictMode>,
    )

    expect(await screen.findByTestId('farmer-login-password')).toHaveTextContent('K7QXM2PA')
    expect(issueCalls()).toHaveLength(1)
  })

  test('says it is creating the login while it waits', () => {
    rpc.mockReturnValue(new Promise(() => {}))
    renderCard(<FarmerLoginCard personId={PERSON} autoIssue />)

    expect(screen.getByTestId('farmer-login-card')).toHaveTextContent('Creating login…')
    expect(screen.queryByTestId('farmer-login-issue')).not.toBeInTheDocument()
  })

  test('a failed first issue offers retry, not a fresh Create button', async () => {
    rpc.mockResolvedValue({ data: null, error: { message: 'this phone number already has an app login' } })
    renderCard(<FarmerLoginCard personId={PERSON} autoIssue />)

    expect(await screen.findByTestId('farmer-login-error')).toHaveTextContent(
      'This phone number already has an app login.',
    )
    expect(screen.getByTestId('farmer-login-retry')).toBeInTheDocument()
    expect(issueCalls()).toHaveLength(1)
  })
})

describe('FarmerLoginCard, when a login exists', () => {
  test('Reset is confirmed first: cancel issues nothing', async () => {
    renderCard(<FarmerLoginCard personId={PERSON} hasLogin />)

    expect(screen.queryByTestId('farmer-login-issue')).not.toBeInTheDocument()
    await userEvent.click(screen.getByTestId('farmer-login-reset'))

    expect(screen.getByTestId('confirm-dialog')).toHaveTextContent("Reset this farmer's app password?")
    await userEvent.click(screen.getByTestId('confirm-dialog-cancel'))

    expect(screen.queryByTestId('confirm-dialog')).not.toBeInTheDocument()
    expect(rpc).not.toHaveBeenCalled()
  })

  test('a confirmed reset shows the new temporary password', async () => {
    rpc.mockResolvedValue({ data: { ...ISSUED, temp_password: 'Z9Y8X7W6', kind: 'reset' }, error: null })
    renderCard(<FarmerLoginCard personId={PERSON} hasLogin />)

    await userEvent.click(screen.getByTestId('farmer-login-reset'))
    await userEvent.click(screen.getByTestId('confirm-dialog-confirm'))

    expect(await screen.findByTestId('farmer-login-password')).toHaveTextContent('Z9Y8X7W6')
    expect(issueCalls()).toHaveLength(1)
  })

  test('the reset button says so while it runs', async () => {
    rpc.mockReturnValue(new Promise(() => {}))
    renderCard(<FarmerLoginCard personId={PERSON} hasLogin />)

    await userEvent.click(screen.getByTestId('farmer-login-reset'))
    await userEvent.click(screen.getByTestId('confirm-dialog-confirm'))

    expect(screen.getByTestId('farmer-login-reset')).toBeDisabled()
    expect(screen.getByTestId('farmer-login-reset')).toHaveTextContent('Resetting…')
  })

  // The "registered, but" sentence belongs to the create path. A refused reset
  // is the database's sentence alone.
  test('a refused reset shows the database message without the registration sentence', async () => {
    rpc.mockResolvedValue({
      data: null,
      error: { message: 'this person signs in as staff: their password cannot be reset here' },
    })
    renderCard(<FarmerLoginCard personId={PERSON} hasLogin />)

    await userEvent.click(screen.getByTestId('farmer-login-reset'))
    await userEvent.click(screen.getByTestId('confirm-dialog-confirm'))

    const failure = await screen.findByTestId('farmer-login-error')
    expect(failure).toHaveTextContent('This person signs in as staff, so their password cannot be reset here.')
    expect(failure).not.toHaveTextContent('The farmer is registered')
    await waitFor(() => expect(screen.getByTestId('farmer-login-retry')).toBeEnabled())
  })
})

describe('FarmerLoginCard never keeps the password', () => {
  test('nothing is written to browser storage', async () => {
    const setItem = vi.spyOn(Storage.prototype, 'setItem')
    rpc.mockResolvedValue({ data: ISSUED, error: null })
    renderCard(<FarmerLoginCard personId={PERSON} autoIssue />)

    await screen.findByTestId('farmer-login-password')

    expect(setItem.mock.calls.flat().join(' ')).not.toContain('K7QXM2PA')
    setItem.mockRestore()
  })
})
