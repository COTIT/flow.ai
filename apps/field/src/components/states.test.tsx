import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'

import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import '@/i18n'

describe('EmptyState', () => {
  // "Zero rows is a legitimate answer. RLS returning nothing means you may not
  // see this. Render an empty state, never an error, and never retry."
  test('states what is absent and offers no retry', () => {
    render(<EmptyState title="No requests yet" detail="Requests appear here once submitted." />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.getByText('No requests yet')).toBeInTheDocument()
    expect(screen.getByText('Requests appear here once submitted.')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /retry|try again/i })).not.toBeInTheDocument()
  })

  test('is not announced as an error', () => {
    render(<EmptyState title="Nothing here" />)
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})

describe('ErrorState', () => {
  // Error contract §9: Postgres messages in this schema are written to be read
  // by humans. Surface them verbatim — never replace one with a generic toast.
  test('surfaces the database message verbatim', () => {
    const message = 'over-commitment: 4100.00 kg available, 4100.00 kg already committed, 100.00 kg requested'
    render(<ErrorState error={new Error(message)} />)

    expect(screen.getByTestId('error-state')).toBeInTheDocument()
    expect(screen.getByText(message)).toBeInTheDocument()
  })

  test('is announced as an alert, unlike an empty state', () => {
    render(<ErrorState error={new Error('boom')} />)
    expect(screen.getByRole('alert')).toBeInTheDocument()
  })

  test('offers retry only when a handler is given', () => {
    const { unmount } = render(<ErrorState error={new Error('boom')} />)
    expect(screen.queryByTestId('error-retry')).not.toBeInTheDocument()
    unmount()

    render(<ErrorState error={new Error('boom')} onRetry={() => {}} />)
    expect(screen.getByTestId('error-retry')).toBeInTheDocument()
  })


  /**
   * QA #4, #20, #25. "Surface them verbatim" is right about the messages the
   * SCHEMA writes, and was being applied to machine noise as well —
   * `pue_request_hours_per_day_check` and `TypeError: Failed to fetch` both
   * reached users as copy.
   */
  test('a check-constraint identifier is replaced by a sentence', () => {
    render(
      <ErrorState
        error={
          new Error(
            'new row for relation "pue_request" violates check constraint "pue_request_hours_per_day_check"',
          )
        }
      />,
    )

    expect(screen.getByTestId('error-state')).toHaveTextContent(/between 0 and 24/i)
    expect(screen.getByTestId('error-state')).not.toHaveTextContent('pue_request')
  })

  test('a JS exception becomes something a field officer can act on', () => {
    render(<ErrorState error={new TypeError('Failed to fetch')} />)

    expect(screen.getByTestId('error-state')).toHaveTextContent(/check your connection/i)
    expect(screen.getByTestId('error-state')).not.toHaveTextContent('TypeError')
  })

  test('a malformed id does not show the parse failure', () => {
    render(<ErrorState error={new Error('invalid input syntax for type uuid: "nope"')} />)

    expect(screen.getByTestId('error-state')).not.toHaveTextContent('uuid')
  })

  // The regression that matters most: the mapping must not eat the messages
  // §9 exists to protect.
  test('and the schema own messages are still untouched', () => {
    const message =
      'over-commitment: 4100.00 kg available, 4100.00 kg already committed, 100.00 kg requested'
    render(<ErrorState error={new Error(message)} />)
    expect(screen.getByText(message)).toBeInTheDocument()
  })

  // The exception, for a user who does not read English: a message the app
  // names exactly is read from the bundle, carrying its values.
  test('a message the app names exactly is shown from the bundle, values intact', () => {
    render(<ErrorState error={new Error('question 4 is required')} />)
    expect(screen.getByText('Question 4 is required.')).toBeInTheDocument()
  })

  test('handles a non-Error thrown value without crashing', () => {
    render(<ErrorState error={'just a string'} />)
    expect(screen.getByText('just a string')).toBeInTheDocument()
  })
})

/**
 * Empty is an answer. Error is a failure. They must not rhyme — that is a
 * specification rule, not a preference, because "you may not see this" and
 * "something broke" ask the user for completely different next moves.
 */
describe('empty and error are not interchangeable', () => {
  test('empty is sunken sand with a dashed mark, and nothing red', () => {
    render(<EmptyState title="No matching supply" detail="That is an honest zero." />)
    const empty = screen.getByTestId('empty-state')

    expect(empty.querySelector('[data-mark="empty"]'), 'the dashed ring mark').not.toBeNull()
    const signature = `${empty.className}|${empty.getAttribute('style') ?? ''}`
    expect(signature).not.toMatch(/flag|destructive|red/i)
  })

  test('error carries the red left rule and the bang', () => {
    render(<ErrorState error={new Error('boom')} />)
    const error = screen.getByTestId('error-state')

    expect(error.querySelector('[data-mark="error"]'), 'the filled disc and bang').not.toBeNull()
    expect(error.getAttribute('style') ?? '').toMatch(/border-left:\s*4px solid var\(--flag-ink\)/)
  })

  test('the two share no visual signature', () => {
    const { unmount } = render(<EmptyState title="Nothing here" />)
    const empty = screen.getByTestId('empty-state')
    const emptySignature = `${empty.className}|${empty.getAttribute('style') ?? ''}`
    unmount()

    render(<ErrorState error={new Error('boom')} />)
    const error = screen.getByTestId('error-state')
    expect(`${error.className}|${error.getAttribute('style') ?? ''}`).not.toBe(emptySignature)
  })
})

describe('the database sentence is never truncated', () => {
  // The message this has to hold, in full, on a phone. Two lines, and every
  // figure in it is one the user needs.
  const OVER_COMMITMENT =
    'over-commitment: 4100.00 kg available, 4100.00 kg already committed, 100.00 kg requested'

  test('it wraps rather than clipping', () => {
    render(<ErrorState error={new Error(OVER_COMMITMENT)} />)
    const message = screen.getByText(OVER_COMMITMENT)
    const style = message.getAttribute('style') ?? ''

    expect(message.className).not.toMatch(/truncate|line-clamp|whitespace-nowrap/)
    expect(style).not.toMatch(/text-overflow|white-space:\s*nowrap/)
    expect(style, 'figures in the sentence stack against the ones above').toMatch(/tabular-nums/)
  })

  test('the retry control stays optional and is a real button', () => {
    render(<ErrorState error={new Error(OVER_COMMITMENT)} onRetry={() => {}} />)
    expect(screen.getByTestId('error-retry').tagName).toBe('BUTTON')
  })
})
