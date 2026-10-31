import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, test, vi } from 'vitest'

import { ConfirmDialog } from '@/components/ConfirmDialog'

function renderDialog(over: Partial<React.ComponentProps<typeof ConfirmDialog>> = {}) {
  const onCancel = vi.fn()
  const onConfirm = vi.fn()
  render(
    <ConfirmDialog
      title="Confirm"
      detail="Details"
      confirmLabel="Yes"
      cancelLabel="No"
      onCancel={onCancel}
      onConfirm={onConfirm}
      {...over}
    />,
  )
  return { onCancel, onConfirm }
}

describe('ConfirmDialog', () => {
  test('confirms and cancels', async () => {
    const user = userEvent.setup()
    const { onCancel, onConfirm } = renderDialog()
    await user.click(screen.getByTestId('confirm-dialog-confirm'))
    await user.click(screen.getByTestId('confirm-dialog-cancel'))
    expect(onConfirm).toHaveBeenCalledTimes(1)
    expect(onCancel).toHaveBeenCalledTimes(1)
  })

  test('backdrop and Escape cancel', async () => {
    const user = userEvent.setup()
    const { onCancel } = renderDialog()
    await user.click(screen.getByTestId('confirm-dialog-backdrop'))
    expect(onCancel).toHaveBeenCalledTimes(1)

    cleanup()
    renderDialog()
    await user.keyboard('{Escape}')
    expect(onCancel).toHaveBeenCalledTimes(1)
  })

  test('confirming disables controls and ignores Escape', async () => {
    const user = userEvent.setup()
    const { onCancel, onConfirm } = renderDialog({ confirming: true })
    fireEvent.mouseDown(screen.getByRole('alertdialog'))
    fireEvent.mouseDown(screen.getByTestId('confirm-dialog-backdrop'))
    expect(screen.getByTestId('confirm-dialog-cancel')).toBeDisabled()
    expect(screen.getByTestId('confirm-dialog-confirm')).toBeDisabled()
    await user.keyboard('{Escape}')
    await user.click(screen.getByTestId('confirm-dialog-confirm'))
    expect(onCancel).not.toHaveBeenCalled()
    expect(onConfirm).not.toHaveBeenCalled()
  })

  test('backdrop ignores a mousedown whose target is the dialog', () => {
    const { onCancel } = renderDialog()
    const backdrop = screen.getByTestId('confirm-dialog-backdrop')
    const dialog = screen.getByRole('alertdialog')
    fireEvent.mouseDown(backdrop, { target: dialog })
    expect(onCancel).not.toHaveBeenCalled()
  })
})
