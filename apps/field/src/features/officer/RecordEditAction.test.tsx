import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const mutate = vi.fn()
const reset = vi.fn()
const editState = { isPending: false, error: null as Error | null }

vi.mock('@/features/officer/useOfficerEdit', () => ({
  useOfficerEdit: () => ({ mutateAsync: mutate, reset, ...editState }),
}))
// No signed-in owner, and no route to the Supabase client in CI without .env.
vi.mock('@/app/session', () => ({ useSession: () => ({ data: undefined }) }))
vi.mock('@/lib/usePersistentForm', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/usePersistentForm')>()
  return { ...actual, usePersistentForm: vi.fn(actual.usePersistentForm) }
})

const { RecordEditAction } = await import('@/features/officer/RecordEditAction')
const { usePersistentForm } = await import('@/lib/usePersistentForm')

function renderAction(version?: string) {
  const client = new QueryClient()
  return render(
    <QueryClientProvider client={client}>
      <RecordEditAction
        table="farm"
        id="f1"
        title="Edit farm"
        fields={[{ name: 'label', label: 'Farm name' }]}
        initialValues={{ label: 'Old farm' }}
        context={{ villageId: 'v1' }}
        version={version}
      />
    </QueryClientProvider>,
  )
}

beforeEach(() => {
  mutate.mockReset()
  reset.mockReset()
  editState.isPending = false
  editState.error = null
})

describe('RecordEditAction', () => {
  test('opens, preloads, saves, and closes on success', async () => {
    const user = userEvent.setup()
    renderAction()
    await user.click(screen.getByTestId('edit-farm-f1'))
    expect(screen.getByTestId('edit-label')).toHaveValue('Old farm')
    await user.click(screen.getByTestId('officer-edit-save'))
    expect(mutate).toHaveBeenCalledTimes(1)
    await waitFor(() => expect(screen.queryByTestId('officer-edit-dialog')).not.toBeInTheDocument())
  })

  test('cancel resets and closes', async () => {
    const user = userEvent.setup()
    renderAction()
    await user.click(screen.getByTestId('edit-farm-f1'))
    await user.click(screen.getByTestId('officer-edit-cancel'))
    expect(reset).toHaveBeenCalled()
    expect(screen.queryByTestId('officer-edit-dialog')).not.toBeInTheDocument()
  })

  test('versions the dialog draft by the record', async () => {
    const user = userEvent.setup()
    renderAction('2026-09-10T08:00:00Z')
    await user.click(screen.getByTestId('edit-farm-f1'))
    expect(vi.mocked(usePersistentForm)).toHaveBeenLastCalledWith(
      'officer-edit',
      'farm:f1',
      expect.anything(),
      expect.anything(),
      { version: '2026-09-10T08:00:00Z' },
    )
  })
})
