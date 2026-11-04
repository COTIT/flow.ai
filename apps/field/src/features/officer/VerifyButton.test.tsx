import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const onVerify = vi.fn()

const { VerifyButton } = await import('@/features/officer/VerifyButton')
await import('@/i18n')

function renderButton(over: Partial<React.ComponentProps<typeof VerifyButton>> = {}) {
  return render(
    <VerifyButton
      table="person"
      id="p1"
      recordLabel="Daudi Mbwana"
      verification="unverified"
      pending={false}
      onVerify={onVerify}
      {...over}
    />,
  )
}

beforeEach(() => onVerify.mockReset())

describe('VerifyButton wording', () => {
  // The table name is a database identifier; with no label the dialog names the kind of record.
  test('with no label the dialog names the record kind, not the table', async () => {
    const user = userEvent.setup()
    renderButton({ table: 'harvest_report', id: 'h1', recordLabel: undefined })

    await user.click(screen.getByTestId('verify-harvest_report-h1'))
    expect(screen.getByRole('alertdialog')).toHaveTextContent('Harvest figure')
    expect(screen.getByRole('alertdialog')).not.toHaveTextContent('harvest_report')
  })
})

describe('VerifyButton safety', () => {
  test('does not mutate before confirmation and confirms once', async () => {
    const user = userEvent.setup()
    renderButton()

    await user.click(screen.getByTestId('verify-person-p1'))
    expect(screen.getByRole('alertdialog')).toHaveTextContent('Daudi Mbwana')
    expect(onVerify).not.toHaveBeenCalled()

    await user.click(screen.getByTestId('confirm-dialog-confirm'))
    expect(onVerify).toHaveBeenCalledTimes(1)
    expect(onVerify).toHaveBeenCalledWith({ table: 'person', id: 'p1' })
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument()
  })

  test('cancel, Escape, and backdrop do not mutate', async () => {
    const user = userEvent.setup()
    const { unmount } = renderButton()

    await user.click(screen.getByTestId('verify-person-p1'))
    await user.click(screen.getByTestId('confirm-dialog-cancel'))
    expect(onVerify).not.toHaveBeenCalled()

    await user.click(screen.getByTestId('verify-person-p1'))
    await user.keyboard('{Escape}')
    expect(onVerify).not.toHaveBeenCalled()

    await user.click(screen.getByTestId('verify-person-p1'))
    await user.click(screen.getByTestId('confirm-dialog-backdrop'))
    expect(onVerify).not.toHaveBeenCalled()
    unmount()
  })

  test('double confirmation cannot submit twice', async () => {
    const user = userEvent.setup()
    renderButton()
    await user.click(screen.getByTestId('verify-person-p1'))
    const confirm = screen.getByTestId('confirm-dialog-confirm')
    await user.click(confirm)
    await user.click(confirm)
    expect(onVerify).toHaveBeenCalledTimes(1)
  })

  test('verified rows have no action', () => {
    renderButton({ verification: 'verified' })
    expect(screen.queryByTestId('verify-person-p1')).not.toBeInTheDocument()
  })

  test('pending rows remain actionable until submitted', async () => {
    const user = userEvent.setup()
    renderButton({ verification: 'pending' })
    await user.click(screen.getByTestId('verify-person-p1'))
    expect(screen.getByRole('alertdialog')).toBeInTheDocument()
  })

  test('server failure clears submit guard so the record can be retried', async () => {
    const user = userEvent.setup()
    const onVerifyRejects = vi.fn().mockRejectedValue(new Error('server refused'))
    renderButton({ onVerify: onVerifyRejects })

    await user.click(screen.getByTestId('verify-person-p1'))
    await user.click(screen.getByTestId('confirm-dialog-confirm'))
    await Promise.resolve()
    await user.click(screen.getByTestId('verify-person-p1'))
    expect(screen.getByRole('alertdialog')).toBeInTheDocument()
    expect(onVerifyRejects).toHaveBeenCalledTimes(1)
  })

  test('disables the open dialog when mutation becomes pending', async () => {
    const user = userEvent.setup()
    const view = renderButton()
    await user.click(screen.getByTestId('verify-person-p1'))
    view.rerender(
      <VerifyButton
        table="person"
        id="p1"
        recordLabel="Daudi Mbwana"
        verification="unverified"
        pending
        onVerify={onVerify}
      />,
    )
    expect(screen.getByTestId('confirm-dialog-confirm')).toBeDisabled()
    expect(screen.getByTestId('confirm-dialog-confirm')).toHaveTextContent('Verifying')
  })
})
