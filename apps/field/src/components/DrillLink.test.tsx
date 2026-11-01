import { render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

vi.mock('@tanstack/react-router', () => ({
  // Forwards the remaining props: dropping them would swallow data-testid and
  // make every assertion here fail for a reason that has nothing to do with
  // the component.
  Link: ({
    children,
    to,
    params,
    ...rest
  }: {
    children: React.ReactNode
    to: string
    params?: Record<string, string>
  } & Record<string, unknown>) => (
    <a
      href={Object.entries(params ?? {}).reduce((acc, [k, v]) => acc.replace(`$${k}`, v), to)}
      {...rest}
    >
      {children}
    </a>
  ),
}))

const { DrillLink } = await import('@/components/DrillLink')
await import('@/i18n')

/**
 * Spec §9.1: "Turns any aggregate cell into a path to its rows."
 *
 * Every headline must have a path back to the records under it — that is the
 * product's traceability claim, and a figure with no route to its rows cannot
 * support it.
 */
describe('DrillLink', () => {
  test('a person drills to that person record', () => {
    render(<DrillLink kind="person" id="p1">Neema Mwakalinga</DrillLink>)
    const link = screen.getByTestId('drill-link')
    expect(link).toHaveAttribute('href', '/officer/people/p1')
    expect(link).toHaveTextContent('Neema Mwakalinga')
  })

  test('a request drills to the ops review screen', () => {
    render(<DrillLink kind="request" id="r1">15.000 kW</DrillLink>)
    expect(screen.getByTestId('drill-link')).toHaveAttribute('href', '/ops/requests/r1')
  })

  test('a demand drills to its match table', () => {
    render(<DrillLink kind="demand" id="d1">9,000.00 kg</DrillLink>)
    expect(screen.getByTestId('drill-link')).toHaveAttribute('href', '/ops/demand/d1')
  })

  test('an opportunity drills to its supply lines', () => {
    render(<DrillLink kind="opportunity" id="o1">6,400.00 kg</DrillLink>)
    expect(screen.getByTestId('drill-link')).toHaveAttribute('href', '/ops/opportunities/o1')
  })

  test('a cycle drills to the officer cycle detail', () => {
    render(<DrillLink kind="cycle" id="c1">Mahindi</DrillLink>)
    expect(screen.getByTestId('drill-link')).toHaveAttribute('href', '/officer/cycles/c1')
  })

  test('a farm drills to the officer farm detail', () => {
    render(<DrillLink kind="farm" id="f1">Shamba la Neema</DrillLink>)
    expect(screen.getByTestId('drill-link')).toHaveAttribute('href', '/officer/farms/f1')
  })

  // A figure with nothing behind it must not pretend to be a path.
  test('without an id it renders the value as plain text, not a dead link', () => {
    render(<DrillLink kind="person" id={null}>—</DrillLink>)
    expect(screen.queryByTestId('drill-link')).not.toBeInTheDocument()
    expect(screen.getByText('—')).toBeInTheDocument()
  })
})

/**
 * The traceability claim, rendered dozens of times per drill-down: no headline
 * without a path back to the records under it. So the value itself is the link
 * — not a "view" affordance parked beside it — and it carries an arrow, which
 * is the one place in the system an icon appears next to a figure.
 */
describe('DrillLink as the value itself', () => {
  test('the link carries an arrow after the value', () => {
    render(
      <DrillLink kind="person" id="6001">
        12,000.00 kg
      </DrillLink>,
    )
    const link = screen.getByTestId('drill-link')

    expect(link).toHaveTextContent('12,000.00 kg')
    expect(link.querySelector('svg'), 'the arrow-right mark').not.toBeNull()
  })

  test('the arrow is decoration, not part of the accessible name', () => {
    render(
      <DrillLink kind="person" id="6001">
        Amina Sanga
      </DrillLink>,
    )
    const svg = screen.getByTestId('drill-link').querySelector('svg')
    expect(svg?.getAttribute('aria-hidden')).toBe('true')
  })

  test('the underline is the link, so it never reads as decoration', () => {
    render(
      <DrillLink kind="cycle" id="7001">
        3,140.00 kg
      </DrillLink>,
    )
    const style = screen.getByTestId('drill-link').getAttribute('style') ?? ''
    expect(style).toMatch(/border-bottom/)
    expect(style).toMatch(/var\(--primary-ink\)/)
  })
})
