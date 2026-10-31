import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, test, vi } from 'vitest'

// No signed-in owner: the draft is in-memory only, so the form is ready at once.
vi.mock('@/app/session', () => ({ useSession: () => ({ data: undefined }) }))
// The real hook, observed, to see which version the draft is keyed against.
vi.mock('@/lib/usePersistentForm', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/usePersistentForm')>()
  return { ...actual, usePersistentForm: vi.fn(actual.usePersistentForm) }
})

const { OfficerEditDialog } = await import('@/features/officer/OfficerEditDialog')
const { usePersistentForm } = await import('@/lib/usePersistentForm')

const fields = [
  { name: 'given_name', label: 'First name', required: true },
  { name: 'family_name', label: 'Family name', required: true },
]

function renderForm(over: Partial<React.ComponentProps<typeof OfficerEditDialog>> = {}) {
  const onSave = vi.fn()
  const onCancel = vi.fn()
  render(
    <OfficerEditDialog
      table="person"
      title="Edit person"
      fields={fields}
      initialValues={{ given_name: 'Neema', family_name: 'Mwakalinga' }}
      pending={false}
      error={null}
      onSave={onSave}
      onCancel={onCancel}
      {...over}
    />,
  )
  return { onSave, onCancel }
}

describe('OfficerEditDialog', () => {
  test('preloads values and saves edited values', async () => {
    const user = userEvent.setup()
    const { onSave } = renderForm()
    expect(screen.getByTestId('edit-given_name')).toHaveValue('Neema')
    await user.clear(screen.getByTestId('edit-given_name'))
    await user.type(screen.getByTestId('edit-given_name'), 'Asha')
    await user.click(screen.getByTestId('officer-edit-save'))
    expect(onSave).toHaveBeenCalledWith({ given_name: 'Asha', family_name: 'Mwakalinga' })
  })

  test('rejects invalid values and preserves input', async () => {
    const user = userEvent.setup()
    const { onSave } = renderForm()
    await user.clear(screen.getByTestId('edit-given_name'))
    await user.click(screen.getByTestId('officer-edit-save'))
    expect(screen.getByTestId('officer-edit-error')).toHaveTextContent('given_name is required')
    expect(onSave).not.toHaveBeenCalled()
  })

  test('cancel calls the owner and displays server errors', async () => {
    const user = userEvent.setup()
    const { onCancel } = renderForm({ error: new Error('server refused') })
    expect(screen.getByTestId('officer-edit-error')).toHaveTextContent('server refused')
    await user.click(screen.getByTestId('officer-edit-cancel'))
    expect(onCancel).toHaveBeenCalledTimes(1)
  })

  // A rejected save must leave the draft alone: it is cleared only after the
  // server confirmed, and onComplete (which closes the dialog) never runs.
  test('a rejected save keeps the entered values and does not complete', async () => {
    const user = userEvent.setup()
    const onComplete = vi.fn()
    const onSave = vi.fn().mockRejectedValue(new Error('network'))
    renderForm({ onSave, onComplete })
    await user.clear(screen.getByTestId('edit-given_name'))
    await user.type(screen.getByTestId('edit-given_name'), 'Not saved')
    await user.click(screen.getByTestId('officer-edit-save'))
    expect(onSave).toHaveBeenCalledTimes(1)
    expect(onComplete).not.toHaveBeenCalled()
    expect(screen.getByTestId('edit-given_name')).toHaveValue('Not saved')
  })

  test('a confirmed save completes', async () => {
    const user = userEvent.setup()
    const onComplete = vi.fn()
    renderForm({ onSave: vi.fn().mockResolvedValue(undefined), onComplete })
    await user.click(screen.getByTestId('officer-edit-save'))
    expect(onComplete).toHaveBeenCalledTimes(1)
  })

  test('a plot shows its area field; only crop cycles choose a measure', () => {
    renderForm({
      table: 'plot',
      fields: [{ name: 'label', label: 'Plot name' }, { name: 'area_ha', label: 'Plot area', type: 'number' }],
      initialValues: { label: 'Plot', area_ha: '1.5' },
    })
    expect(screen.getByTestId('edit-area_ha')).toHaveValue(1.5)
  })

  test('pending state disables controls and renders select options', () => {
    renderForm({
      pending: true,
      fields: [{ name: 'status', label: 'Status', options: [{ value: 'growing', label: 'Growing' }] }, { name: 'area', label: 'Area', type: 'number' }],
      initialValues: { status: 'growing' },
    })
    expect(screen.getByTestId('edit-status')).toHaveValue('growing')
    expect(screen.getByTestId('officer-edit-save')).toBeDisabled()
    expect(screen.getByTestId('officer-edit-cancel')).toBeDisabled()
    expect(screen.getByTestId('edit-status')).toBeDisabled()
  })

  test('updates a select value', async () => {
    const user = userEvent.setup()
    renderForm({
      fields: [{ name: 'status', label: 'Status', options: [{ value: 'planned', label: 'Planned' }, { value: 'growing', label: 'Growing' }] }],
      initialValues: { status: 'planned' },
    })
    await user.selectOptions(screen.getByTestId('edit-status'), 'growing')
    expect(screen.getByTestId('edit-status')).toHaveValue('growing')
  })

  test('renders an empty select value when a nullable option is absent', () => {
    renderForm({
      fields: [{ name: 'status', label: 'Status', options: [{ value: '', label: 'Choose' }] }],
      initialValues: {},
    })
    expect(screen.getByTestId('edit-status')).toHaveValue('')
  })

  test('switches the visible crop measure field when crop changes', async () => {
    const user = userEvent.setup()
    renderForm({
      table: 'crop_cycle',
      fields: [
        { name: 'crop_id', label: 'Crop', options: [{ value: 'area-crop', label: 'Area crop' }, { value: 'tree-crop', label: 'Tree crop' }] },
        { name: 'area_ha', label: 'Area', type: 'number' },
        { name: 'tree_count', label: 'Trees', type: 'number' },
      ],
      initialValues: { crop_id: 'area-crop', area_ha: '1' },
      measure: 'area',
      measureByCrop: { 'area-crop': 'area', 'tree-crop': 'tree_count' },
    })
    expect(screen.getByTestId('edit-area_ha')).toBeVisible()
    expect(screen.queryByTestId('edit-tree_count')).not.toBeInTheDocument()
    await user.selectOptions(screen.getByTestId('edit-crop_id'), 'tree-crop')
    expect(screen.queryByTestId('edit-area_ha')).not.toBeInTheDocument()
    expect(screen.getByTestId('edit-tree_count')).toBeVisible()
  })

  test('falls back to the current crop measure when no crop map is supplied', () => {
    renderForm({
      table: 'crop_cycle',
      fields: [{ name: 'crop_id', label: 'Crop' }, { name: 'area_ha', label: 'Area' }],
      initialValues: { crop_id: 'area-crop', area_ha: '1' },
      measure: 'area',
    })
    expect(screen.getByTestId('edit-area_ha')).toBeVisible()
  })

  test('falls back when the crop map has no entry for the selected crop', () => {
    renderForm({
      table: 'crop_cycle',
      fields: [{ name: 'crop_id', label: 'Crop' }, { name: 'area_ha', label: 'Area' }],
      initialValues: { crop_id: 'missing-crop', area_ha: '1' },
      measure: 'area',
      measureByCrop: { 'other-crop': 'tree_count' },
    })
    expect(screen.getByTestId('edit-area_ha')).toBeVisible()
  })

  test('handles a missing crop id while editing a cycle', () => {
    renderForm({
      table: 'crop_cycle',
      fields: [{ name: 'crop_id', label: 'Crop' }, { name: 'area_ha', label: 'Area' }],
      initialValues: { area_ha: '1' },
      measure: 'area',
      measureByCrop: { 'area-crop': 'area' },
    })
    expect(screen.getByTestId('edit-area_ha')).toBeVisible()
  })
})

describe('OfficerEditDialog labels', () => {
  // Officer surfaces ship Swahili, so titles, labels and enum options are keys.
  test('title, field labels and enum options resolve through i18n', async () => {
    await import('@/i18n')
    renderForm({
      table: 'crop_cycle',
      title: 'officerEdit.titles.crop_cycle',
      fields: [
        { name: 'season_label', label: 'officerEdit.fields.season_label' },
        { name: 'status', label: 'officerEdit.fields.status', options: [{ value: 'growing', label: 'cycleStatus.growing' }, { value: 'x', label: 'Mahindi' }] },
      ],
      initialValues: { season_label: '', status: 'growing' },
    })
    const dialog = screen.getByTestId('officer-edit-dialog')
    expect(dialog).not.toHaveTextContent('officerEdit.')
    expect(dialog).not.toHaveTextContent('cycleStatus.')
    // A database name is shown as written.
    expect(screen.getByRole('option', { name: 'Mahindi' })).toBeInTheDocument()
  })

  // An abandoned draft must not resurrect over a record changed since.
  test('versions the draft by the record it edits', () => {
    renderForm({ recordId: 'p1', version: '2026-09-10T08:00:00Z' })
    expect(vi.mocked(usePersistentForm)).toHaveBeenLastCalledWith(
      'officer-edit',
      'person:p1',
      expect.anything(),
      expect.anything(),
      { version: '2026-09-10T08:00:00Z' },
    )
  })
})
