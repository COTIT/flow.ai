import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const rpc = vi.fn()

vi.mock('@/lib/supabase', () => ({ supabase: { rpc: (...args: unknown[]) => rpc(...args) } }))

/**
 * The camera is its own component with its own tests. Here it is a stand-in
 * that lets a test decide what the camera saw.
 */
vi.mock('@/features/officer/QrScanner', () => ({
  QrScanner: ({ onResult, onUnavailable }: { onResult: (text: string) => void; onUnavailable: () => void }) => (
    <div data-testid="fake-scanner">
      <button type="button" data-testid="fake-scan-voucher" onClick={() => onResult('R360V:K7QXM2PA9D')}>
        voucher
      </button>
      <button type="button" data-testid="fake-scan-other" onClick={() => onResult('https://example.org')}>
        other
      </button>
      <button type="button" data-testid="fake-scan-unavailable" onClick={onUnavailable}>
        unavailable
      </button>
    </div>
  ),
}))

const { RedeemScreen } = await import('@/features/officer/RedeemScreen')
const { default: i18n } = await import('@/i18n')

const CODE = 'K7QXM2PA9D'
const VOUCHER = '22222222-2222-4222-8222-222222222222'

function preview(over: Record<string, unknown> = {}) {
  return {
    found: true,
    voucher_id: VOUCHER,
    status: 'issued',
    expired: false,
    amount: 5000,
    currency: 'TZS',
    issued_at: '2026-09-20T07:00:00Z',
    expires_at: '2026-10-20T07:00:00Z',
    redeemed_at: null,
    redeemed_by_name: null,
    void_reason: null,
    survey_title_en: 'Irrigation interest',
    survey_title_sw: 'Nia ya umwagiliaji',
    household_label: 'Kaya ya Mwakalinga',
    head_name: 'Neema Mwakalinga',
    respondent_name: 'Daudi Mwakalinga',
    needs_ops: false,
    can_redeem: true,
    blocked_reason: null,
    ...over,
  }
}

const REDEEMED = {
  voucher_id: VOUCHER,
  status: 'redeemed',
  redeemed_at: '2026-09-29T07:42:00Z',
  redeemed_by_name: 'Juma Officer',
  amount: 5000,
  currency: 'TZS',
  replayed: false,
}

const TIMELINE = [
  { occurred_at: '2026-09-20T07:00:00Z', kind: 'issued', actor_name: 'Daudi Mwakalinga', actor_role: 'farmer', detail: null },
  { occurred_at: '2026-09-29T07:41:00Z', kind: 'scanned', actor_name: 'Juma Officer', actor_role: 'field_officer', detail: null },
  {
    occurred_at: '2026-09-29T07:42:00Z',
    kind: 'redeemed',
    actor_name: 'Juma Officer',
    actor_role: 'field_officer',
    detail: { id_type_seen: 'nida' },
  },
]

type Reply = { data: unknown; error: { message: string } | null } | Promise<never>
let replies: Record<string, Reply | Reply[]>

function reply(name: string): Reply {
  const configured = replies[name]
  if (Array.isArray(configured)) return configured.shift() ?? { data: null, error: { message: `no reply for ${name}` } }
  return configured ?? { data: null, error: { message: `no reply for ${name}` } }
}

const calls = (name: string) => rpc.mock.calls.filter(([called]) => called === name)

function renderScreen() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  return render(
    <QueryClientProvider client={queryClient}>
      <RedeemScreen />
    </QueryClientProvider>,
  )
}

async function lookUp(code = 'k7qxm-2pa9d') {
  const user = userEvent.setup()
  await user.type(screen.getByTestId('redeem-code'), code)
  await user.click(screen.getByTestId('redeem-lookup'))
  return user
}

beforeEach(async () => {
  await i18n.changeLanguage('en')
  rpc.mockReset()
  rpc.mockImplementation((name: string) => Promise.resolve(reply(name)))
  replies = {
    app_voucher_lookup: { data: preview(), error: null },
    app_voucher_redeem: { data: REDEEMED, error: null },
    app_voucher_timeline: { data: TIMELINE, error: null },
  }
})

describe('RedeemScreen entry', () => {
  test('offers the camera and an always-visible code field, and looks nothing up yet', () => {
    renderScreen()

    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Redeem a voucher')
    expect(screen.getByTestId('redeem-scan')).toHaveTextContent('Scan QR code')
    expect(screen.getByTestId('redeem-code')).toBeVisible()
    expect(screen.getByTestId('redeem-lookup')).toHaveTextContent('Look up')
    expect(screen.queryByTestId('fake-scanner')).not.toBeInTheDocument()
    expect(rpc).not.toHaveBeenCalled()
  })

  // A voucher code must never be remembered by the browser.
  test('the code field is not autofilled or remembered', () => {
    renderScreen()
    expect(screen.getByTestId('redeem-code')).toHaveAttribute('autocomplete', 'off')
  })

  // Every lookup is an audit event, so something that is not a code at all is
  // stopped on the device and never reaches the database.
  test('something that is not a voucher code is refused without a lookup', async () => {
    renderScreen()
    await lookUp('hello')

    expect(screen.getByTestId('redeem-not-a-voucher')).toHaveTextContent('That is not a Ruaha voucher code.')
    expect(rpc).not.toHaveBeenCalled()
  })

  test('a typed code is normalised before it is looked up', async () => {
    renderScreen()
    await lookUp('k7qxm-2pa9d')

    await screen.findByTestId('redeem-preview')
    expect(calls('app_voucher_lookup')).toEqual([['app_voucher_lookup', { p_code: CODE }]])
  })

  test('the lookup button says so while it waits, and cannot be pressed twice', async () => {
    replies.app_voucher_lookup = new Promise<never>(() => {})
    renderScreen()
    const user = await lookUp()

    expect(screen.getByTestId('redeem-lookup')).toBeDisabled()
    expect(screen.getByTestId('redeem-lookup')).toHaveTextContent('Looking up…')
    await user.click(screen.getByTestId('redeem-lookup'))
    expect(calls('app_voucher_lookup')).toHaveLength(1)
  })

  test('submitting the form with Enter looks the code up', async () => {
    renderScreen()
    const user = userEvent.setup()
    await user.type(screen.getByTestId('redeem-code'), `${CODE}{Enter}`)

    await screen.findByTestId('redeem-preview')
  })
})

describe('RedeemScreen camera', () => {
  test('Scan opens the camera and the same button closes it', async () => {
    renderScreen()
    const user = userEvent.setup()

    await user.click(screen.getByTestId('redeem-scan'))
    expect(screen.getByTestId('fake-scanner')).toBeInTheDocument()
    expect(screen.getByTestId('redeem-scan')).toHaveTextContent('Stop camera')

    await user.click(screen.getByTestId('redeem-scan'))
    expect(screen.queryByTestId('fake-scanner')).not.toBeInTheDocument()
  })

  test('a scan closes the camera and goes straight to the lookup', async () => {
    renderScreen()
    const user = userEvent.setup()

    await user.click(screen.getByTestId('redeem-scan'))
    await user.click(screen.getByTestId('fake-scan-voucher'))

    expect(screen.queryByTestId('fake-scanner')).not.toBeInTheDocument()
    await screen.findByTestId('redeem-preview')
    expect(calls('app_voucher_lookup')).toEqual([['app_voucher_lookup', { p_code: CODE }]])
  })

  test('a scan of some other QR code is "not a voucher", with no lookup', async () => {
    renderScreen()
    const user = userEvent.setup()

    await user.click(screen.getByTestId('redeem-scan'))
    await user.click(screen.getByTestId('fake-scan-other'))

    expect(screen.getByTestId('redeem-not-a-voucher')).toBeInTheDocument()
    expect(rpc).not.toHaveBeenCalled()
  })

  test('no camera closes the scanner and points to the typed code', async () => {
    renderScreen()
    const user = userEvent.setup()

    await user.click(screen.getByTestId('redeem-scan'))
    await user.click(screen.getByTestId('fake-scan-unavailable'))

    expect(screen.queryByTestId('fake-scanner')).not.toBeInTheDocument()
    expect(screen.getByTestId('redeem-camera-unavailable')).toHaveTextContent('Type the code instead.')
    expect(screen.getByTestId('redeem-code')).toBeVisible()
  })

  test('opening the camera again clears the unavailable notice', async () => {
    renderScreen()
    const user = userEvent.setup()

    await user.click(screen.getByTestId('redeem-scan'))
    await user.click(screen.getByTestId('fake-scan-unavailable'))
    await user.click(screen.getByTestId('redeem-scan'))

    expect(screen.queryByTestId('redeem-camera-unavailable')).not.toBeInTheDocument()
  })
})

describe('RedeemScreen preview', () => {
  // An unknown code and one outside the officer's villages are the same
  // answer on purpose. It is an answer, not an error.
  test('found:false says no voucher, without an alert', async () => {
    replies.app_voucher_lookup = { data: { found: false }, error: null }
    renderScreen()
    await lookUp()

    expect(await screen.findByTestId('redeem-not-found')).toHaveTextContent(
      'No voucher with this code in your villages.',
    )
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  test('shows who, what, how much and until when — before anything is handed over', async () => {
    renderScreen()
    await lookUp()

    const card = await screen.findByTestId('redeem-preview')
    expect(card).toHaveTextContent('K7QXM-2PA9D')
    expect(card).toHaveTextContent('Kaya ya Mwakalinga')
    expect(card).toHaveTextContent('Neema Mwakalinga')
    expect(card).toHaveTextContent('Daudi Mwakalinga')
    expect(card).toHaveTextContent('Irrigation interest')
    expect(screen.getByTestId('redeem-amount')).toHaveTextContent('TZS 5,000.00')
    expect(within(card).getByTestId('status-pill')).toHaveTextContent('Not yet collected')
    expect(card).toHaveTextContent('Valid until 20 Oct 2026')
    expect(screen.queryByTestId('redeem-needs-ops')).not.toBeInTheDocument()
    expect(screen.getByTestId('redeem-confirm')).toHaveTextContent('Confirm and hand over TZS 5,000.00')
    expect(calls('app_voucher_redeem')).toHaveLength(0)
  })

  test('the survey title follows the language', async () => {
    renderScreen()
    await lookUp()
    await screen.findByTestId('redeem-preview')

    await i18n.changeLanguage('sw')
    await waitFor(() => expect(screen.getByTestId('redeem-preview')).toHaveTextContent('Nia ya umwagiliaji'))
  })

  test('missing names render as a dash rather than as nothing', async () => {
    replies.app_voucher_lookup = { data: preview({ head_name: null, respondent_name: null }), error: null }
    renderScreen()
    await lookUp()

    expect(await screen.findByTestId('redeem-head')).toHaveTextContent('—')
    expect(screen.getByTestId('redeem-respondent')).toHaveTextContent('—')
  })

  test('an audit hold is announced', async () => {
    replies.app_voucher_lookup = { data: preview({ needs_ops: true }), error: null }
    renderScreen()
    await lookUp()

    expect(await screen.findByTestId('redeem-needs-ops')).toHaveTextContent(
      'Held for audit: ops or admin must redeem this voucher in person.',
    )
  })

  // The database decided, and its sentence says why. No form is offered.
  test('a blocked voucher shows the reason and no hand-over form', async () => {
    replies.app_voucher_lookup = {
      data: preview({
        can_redeem: false,
        needs_ops: true,
        blocked_reason: 'this voucher is held for an audit: ops or admin must redeem it in person',
      }),
      error: null,
    }
    renderScreen()
    await lookUp()

    const blocked = await screen.findByTestId('redeem-blocked')
    expect(blocked).toHaveTextContent('You cannot redeem this voucher')
    expect(blocked).toHaveTextContent('This voucher is held for an audit: ops or admin must redeem it in person')
    expect(screen.queryByTestId('redeem-confirm')).not.toBeInTheDocument()
    expect(screen.queryByTestId('redeem-id-type')).not.toBeInTheDocument()
  })

  test('a blocked voucher with no stated reason still says it cannot be redeemed', async () => {
    replies.app_voucher_lookup = { data: preview({ can_redeem: false, blocked_reason: null }), error: null }
    renderScreen()
    await lookUp()

    expect(await screen.findByTestId('redeem-blocked')).toHaveTextContent('You cannot redeem this voucher')
  })

  test('an expired voucher is shown as expired, not as waiting', async () => {
    replies.app_voucher_lookup = {
      data: preview({ expired: true, can_redeem: false, blocked_reason: 'this voucher expired on 20 Oct 2026' }),
      error: null,
    }
    renderScreen()
    await lookUp()

    const card = await screen.findByTestId('redeem-preview')
    expect(within(card).getByTestId('status-pill')).toHaveTextContent('Expired')
  })

  test('a redeemed voucher shows as collected', async () => {
    replies.app_voucher_lookup = {
      data: preview({
        status: 'redeemed',
        can_redeem: false,
        blocked_reason: 'voucher already redeemed on 01 Oct 2026 10:42 by Juma Officer',
      }),
      error: null,
    }
    renderScreen()
    await lookUp()

    const card = await screen.findByTestId('redeem-preview')
    expect(within(card).getByTestId('status-pill')).toHaveTextContent('Collected')
    expect(screen.getByTestId('redeem-blocked')).toHaveTextContent(
      'This voucher was already redeemed on 1 Oct 2026, 10:42 by Juma Officer.',
    )
  })

  test('a failed lookup is an error, verbatim', async () => {
    replies.app_voucher_lookup = { data: null, error: { message: 'only staff may look up vouchers' } }
    renderScreen()
    await lookUp()

    expect(await screen.findByTestId('error-state')).toHaveTextContent('You do not have permission to do that.')
    expect(screen.queryByTestId('redeem-preview')).not.toBeInTheDocument()
  })

  test('a second lookup replaces the first preview', async () => {
    replies.app_voucher_lookup = [
      { data: preview(), error: null },
      { data: { found: false }, error: null },
    ]
    renderScreen()
    const user = await lookUp()
    await screen.findByTestId('redeem-preview')

    await user.click(screen.getByTestId('redeem-lookup'))
    await screen.findByTestId('redeem-not-found')
    expect(screen.queryByTestId('redeem-preview')).not.toBeInTheDocument()
  })

  test('a non-code after a preview clears the preview', async () => {
    renderScreen()
    const user = await lookUp()
    await screen.findByTestId('redeem-preview')

    await user.clear(screen.getByTestId('redeem-code'))
    await user.type(screen.getByTestId('redeem-code'), 'nope')
    await user.click(screen.getByTestId('redeem-lookup'))

    expect(screen.getByTestId('redeem-not-a-voucher')).toBeInTheDocument()
    expect(screen.queryByTestId('redeem-preview')).not.toBeInTheDocument()
  })
})

describe('RedeemScreen hand-over', () => {
  async function chooseId(user: ReturnType<typeof userEvent.setup>, label: string) {
    await user.click(screen.getByTestId('redeem-id-type'))
    await user.click(await screen.findByRole('option', { name: label }))
  }

  test('the ID choices are the four document types', async () => {
    renderScreen()
    const user = await lookUp()
    await screen.findByTestId('redeem-preview')

    expect(screen.getByTestId('redeem-id-type')).toHaveTextContent('Choose the document')
    await user.click(screen.getByTestId('redeem-id-type'))
    const options = await screen.findAllByRole('option')
    expect(options.map((option) => option.textContent)).toEqual([
      'NIDA card',
      'Voter card',
      'Driving licence',
      'Village letter',
    ])
  })

  test('confirm sends the code, the ID seen and the name check, then shows the result and the trail', async () => {
    renderScreen()
    const user = await lookUp()
    await screen.findByTestId('redeem-preview')

    await chooseId(user, 'Voter card')
    expect(screen.getByTestId('redeem-id-type')).toHaveTextContent('Voter card')
    await user.click(screen.getByTestId('redeem-name-confirm'))
    await user.click(screen.getByTestId('redeem-confirm'))

    const done = await screen.findByTestId('redeem-done')
    expect(calls('app_voucher_redeem')).toEqual([
      ['app_voucher_redeem', { p_code: CODE, p_id_type: 'voter', p_name_confirmed: true }],
    ])
    expect(done).toHaveTextContent('Redeemed')
    expect(done).toHaveTextContent('TZS 5,000.00 handed over. This voucher cannot be used again.')

    expect(await screen.findByTestId('audit-timeline')).toBeInTheDocument()
    expect(calls('app_voucher_timeline')).toEqual([['app_voucher_timeline', { p_voucher_id: VOUCHER }]])
    expect(screen.getByTestId('audit-event-redeemed')).toHaveTextContent('ID checked: NIDA card')
    // The form is gone: nothing on this screen can be pressed twice.
    expect(screen.queryByTestId('redeem-confirm')).not.toBeInTheDocument()
    expect(screen.queryByTestId('redeem-code')).not.toBeInTheDocument()
  })

  // Do not pre-validate what the database enforces: an unchosen ID and an
  // unticked box are sent, and the database's sentence comes back.
  test('the database refuses a missing ID in its own words, and the preview stays', async () => {
    replies.app_voucher_redeem = { data: null, error: { message: 'record which ID document you checked' } }
    renderScreen()
    const user = await lookUp()
    await screen.findByTestId('redeem-preview')

    await user.click(screen.getByTestId('redeem-confirm'))

    expect(await screen.findByRole('alert')).toHaveTextContent('Record which ID document you checked.')
    expect(calls('app_voucher_redeem')).toEqual([
      ['app_voucher_redeem', { p_code: CODE, p_id_type: null, p_name_confirmed: false }],
    ])
    expect(screen.getByTestId('redeem-preview')).toBeInTheDocument()
    expect(screen.getByTestId('redeem-confirm')).toBeEnabled()
  })

  test('a four-eyes refusal is shown verbatim', async () => {
    replies.app_voucher_redeem = {
      data: null,
      error: { message: 'you verified this household, so another staff member must redeem this voucher' },
    }
    renderScreen()
    const user = await lookUp()
    await screen.findByTestId('redeem-preview')
    await chooseId(user, 'NIDA card')
    await user.click(screen.getByTestId('redeem-name-confirm'))
    await user.click(screen.getByTestId('redeem-confirm'))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'You verified this household, so another staff member must redeem this voucher.',
    )
  })

  test('the confirm button says so while it runs', async () => {
    replies.app_voucher_redeem = new Promise<never>(() => {})
    renderScreen()
    const user = await lookUp()
    await screen.findByTestId('redeem-preview')
    await user.click(screen.getByTestId('redeem-confirm'))

    expect(screen.getByTestId('redeem-confirm')).toBeDisabled()
    expect(screen.getByTestId('redeem-confirm')).toHaveTextContent('Redeeming…')
  })

  test('the name check can be unticked again', async () => {
    renderScreen()
    const user = await lookUp()
    await screen.findByTestId('redeem-preview')

    await user.click(screen.getByTestId('redeem-name-confirm'))
    expect(screen.getByTestId('redeem-name-confirm')).toBeChecked()
    await user.click(screen.getByTestId('redeem-name-confirm'))
    expect(screen.getByTestId('redeem-name-confirm')).not.toBeChecked()
  })

  test('Redeem another returns to an empty entry', async () => {
    renderScreen()
    const user = await lookUp()
    await screen.findByTestId('redeem-preview')
    await user.click(screen.getByTestId('redeem-confirm'))
    await screen.findByTestId('redeem-done')

    await user.click(screen.getByTestId('redeem-another'))

    expect(screen.queryByTestId('redeem-done')).not.toBeInTheDocument()
    expect(screen.queryByTestId('redeem-preview')).not.toBeInTheDocument()
    expect(screen.getByTestId('redeem-code')).toHaveValue('')
  })

  test('a failed trail read is an error, with a retry that reads it again', async () => {
    replies.app_voucher_timeline = [
      { data: null, error: { message: 'timeline failed' } },
      { data: TIMELINE, error: null },
    ]
    renderScreen()
    const user = await lookUp()
    await screen.findByTestId('redeem-preview')
    await user.click(screen.getByTestId('redeem-confirm'))

    await screen.findByTestId('redeem-done')
    expect(await screen.findByText('timeline failed')).toBeInTheDocument()
    // The hand-over itself succeeded: a failed trail read does not undo it.
    expect(screen.getByTestId('redeem-done')).toHaveTextContent('Redeemed')

    await user.click(screen.getByTestId('error-retry'))
    expect(await screen.findByTestId('audit-timeline')).toBeInTheDocument()
    expect(calls('app_voucher_timeline')).toHaveLength(2)
  })

  test('the trail shows loading until it arrives', async () => {
    replies.app_voucher_timeline = new Promise<never>(() => {})
    renderScreen()
    const user = await lookUp()
    await screen.findByTestId('redeem-preview')
    await user.click(screen.getByTestId('redeem-confirm'))

    await screen.findByTestId('redeem-done')
    expect(screen.getByTestId('redeem-timeline-loading')).toBeInTheDocument()
  })
})
