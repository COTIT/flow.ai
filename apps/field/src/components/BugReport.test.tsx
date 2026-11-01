import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, expect, test, vi } from 'vitest'

const captureScreen = vi.fn()
const submitBugReport = vi.fn()
vi.mock('@/lib/bugReports', () => ({ captureScreen: () => captureScreen(), submitBugReport: (i: unknown) => submitBugReport(i) }))

const { BugReportButton } = await import('@/components/BugReport')
const { clearTrail, recordTrail } = await import('@/lib/bugTrail')
await import('@/i18n')

const ME = '80000000-0000-4000-8000-000000000005'

function renderButton() {
  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { mutations: { retry: false } } })}>
      <BugReportButton reporterId={ME} />
    </QueryClientProvider>,
  )
}

beforeEach(() => {
  clearTrail()
  captureScreen.mockReset().mockResolvedValue(new Blob(['png'], { type: 'image/png' }))
  submitBugReport.mockReset().mockResolvedValue(undefined)
  URL.createObjectURL = vi.fn(() => 'blob:preview')
  URL.revokeObjectURL = vi.fn()
})

test('the button opens a sheet holding the screenshot and the steps taken before the tap', async () => {
  recordTrail('nav', '(start) → /farm')
  recordTrail('click', 'link "Surveys" [nav-surveys]')
  renderButton()
  await userEvent.click(screen.getByTestId('bug-fab'))

  expect(await screen.findByTestId('bug-sheet')).toBeInTheDocument()
  expect(screen.getByTestId('bug-screenshot')).toHaveAttribute('src', 'blob:preview')
  expect(screen.getByTestId('bug-actions')).toHaveTextContent('Recent steps attached: 2')
  expect(screen.getByTestId('bug-actions')).toHaveTextContent('link "Surveys" [nav-surveys]')
})

test('a description is required', async () => {
  renderButton()
  await userEvent.click(screen.getByTestId('bug-fab'))
  await userEvent.click(await screen.findByTestId('bug-submit'))

  expect(screen.getByTestId('bug-description-error')).toHaveTextContent('Describe what went wrong.')
  expect(submitBugReport).not.toHaveBeenCalled()
})

test('without a screenshot a photo is required, and with one the report goes', async () => {
  captureScreen.mockResolvedValue(null)
  renderButton()
  await userEvent.click(screen.getByTestId('bug-fab'))
  expect(await screen.findByTestId('bug-screenshot-failed')).toBeInTheDocument()

  await userEvent.type(screen.getByTestId('bug-description'), 'Survey list stays empty')
  await userEvent.click(screen.getByTestId('bug-submit'))
  expect(screen.getByTestId('bug-image-error')).toBeInTheDocument()
  expect(submitBugReport).not.toHaveBeenCalled()

  const photo = new File(['jpg'], 'screen.jpg', { type: 'image/jpeg' })
  await userEvent.upload(screen.getByTestId('bug-photo'), photo)
  expect(screen.getByTestId('bug-photo-preview')).toBeInTheDocument()
  await userEvent.click(screen.getByTestId('bug-submit'))

  await waitFor(() =>
    expect(submitBugReport).toHaveBeenCalledWith(
      expect.objectContaining({ reporterId: ME, description: 'Survey list stays empty', screenshot: null, photo }),
    ),
  )
  expect(await screen.findByTestId('bug-sent')).toHaveTextContent('Thank you. Your report was sent.')
  await waitFor(() => expect(screen.queryByTestId('bug-sheet')).not.toBeInTheDocument())
})

test('a refused report keeps the sheet open with the server message', async () => {
  submitBugReport.mockRejectedValue(new Error('The object exceeded the maximum allowed size'))
  renderButton()
  await userEvent.click(screen.getByTestId('bug-fab'))
  await userEvent.type(await screen.findByTestId('bug-description'), 'x')
  await userEvent.click(screen.getByTestId('bug-submit'))

  expect(await screen.findByTestId('bug-send-error')).toHaveTextContent('The object exceeded the maximum allowed size')
  expect(screen.getByTestId('bug-sheet')).toBeInTheDocument()
})
