import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'

import { CoverageBar } from '@/components/CoverageBar'
import '@/i18n'

/**
 * Spec §9.1: "Demand against available supply, with the already-committed
 * slice visible."
 *
 * The seeded case: Ilundo maize has 12,000 kg expected, 6,400 kg already
 * committed to the demo opportunity, so 5,600 kg available against a
 * 9,000 kg demand — 62.2% coverage.
 */
const seeded = { demandKg: 9000, availableKg: 5600, committedKg: 6400, coveragePct: 62.2 }

describe('CoverageBar', () => {
  test('states the coverage the view calculated', () => {
    render(<CoverageBar {...seeded} />)
    expect(screen.getByTestId('coverage-pct')).toHaveTextContent('62.2%')
  })

  test('shows the demand and the available supply as figures', () => {
    render(<CoverageBar {...seeded} />)
    expect(screen.getByTestId('coverage-demand')).toHaveTextContent('9,000.00 kg')
    expect(screen.getByTestId('coverage-available')).toHaveTextContent('5,600.00 kg')
  })

  // The committed slice is the point of the component: supply that looks
  // available in a total is not available if it is already promised.
  test('the already-committed slice is visible', () => {
    render(<CoverageBar {...seeded} />)
    expect(screen.getByTestId('coverage-committed')).toHaveTextContent('6,400.00 kg')
  })

  test('the bar is described for assistive tech, not just drawn', () => {
    render(<CoverageBar {...seeded} />)
    const bar = screen.getByRole('meter')
    expect(bar).toHaveAttribute('aria-valuenow', '62.2')
    expect(bar).toHaveAttribute('aria-valuemin', '0')
    expect(bar).toHaveAttribute('aria-valuemax', '100')
  })

  // The seeded coffee demand has no matching supply at all. That is an honest
  // zero, and must read as one rather than as a missing bar.
  test('zero available supply reads as an explicit zero', () => {
    render(<CoverageBar demandKg={4000} availableKg={0} committedKg={0} coveragePct={0} />)
    expect(screen.getByTestId('coverage-pct')).toHaveTextContent('0.0%')
    expect(screen.getByTestId('coverage-available')).toHaveTextContent('0.00 kg')
  })

  test('supply beyond the demand caps the bar at full without misreporting it', () => {
    render(<CoverageBar demandKg={1000} availableKg={5000} committedKg={0} coveragePct={100} />)
    expect(screen.getByTestId('coverage-pct')).toHaveTextContent('100.0%')
    expect(screen.getByRole('meter')).toHaveAttribute('aria-valuenow', '100')
  })

  test('a null coverage is unknown, not zero', () => {
    render(
      <CoverageBar demandKg={9000} availableKg={null} committedKg={null} coveragePct={null} />,
    )
    expect(screen.getByTestId('coverage-pct')).toHaveTextContent('—')
    expect(screen.queryByRole('meter')).not.toBeInTheDocument()
  })

  // The client never computes coverage: v_demand_match does. The component
  // renders what it was handed, so a mismatch is a view bug, not a UI bug.
  test('renders the coverage it was given even when it disagrees with the figures', () => {
    render(<CoverageBar demandKg={9000} availableKg={5600} committedKg={0} coveragePct={41.5} />)
    expect(screen.getByTestId('coverage-pct')).toHaveTextContent('41.5%')
  })
})

/**
 * Three quantities, distinguished three ways: solid fill, a lighter tone, and
 * an outlined swatch drawn OUTSIDE the track. Committed supply is not available
 * and no stacked bar may imply that it is — which is the whole reason this
 * component exists rather than a two-segment progress bar.
 */
describe('CoverageBar tells its three quantities apart', () => {
  test('the track holds available and not-covered, and nothing else', () => {
    render(<CoverageBar {...seeded} />)
    const track = screen.getByRole('meter')

    const segments = track.querySelectorAll('[data-segment]')
    expect([...segments].map((s) => s.getAttribute('data-segment'))).toEqual([
      'available',
      'uncovered',
    ])
  })

  test('available and not-covered are both solid, in different tones', () => {
    render(<CoverageBar {...seeded} />)
    const track = screen.getByRole('meter')

    const available = track.querySelector('[data-segment="available"]')?.getAttribute('style') ?? ''
    const uncovered = track.querySelector('[data-segment="uncovered"]')?.getAttribute('style') ?? ''

    expect(available).toMatch(/background:\s*var\(--accent\)/)
    expect(uncovered).toMatch(/background:\s*var\(--sand-2\)/)
    expect(uncovered).not.toMatch(/gradient|hatch/)
  })

  // The claim the design exists to prevent: committed supply looking like part
  // of what is covered.
  test('committed is drawn outside the track', () => {
    render(<CoverageBar {...seeded} />)
    const track = screen.getByRole('meter')
    const committed = screen.getByTestId('coverage-committed')

    expect(track.contains(committed), 'committed supply is not part of the bar').toBe(false)
    expect(screen.getByTestId('coverage-committed')).toHaveTextContent('6,400.00 kg')
  })

  test('every legend row carries its own swatch and figure', () => {
    render(<CoverageBar {...seeded} />)
    const legend = screen.getByTestId('coverage-legend')

    expect(legend.querySelectorAll('[data-swatch]')).toHaveLength(3)
    expect([...legend.querySelectorAll('[data-swatch]')].map((s) => s.getAttribute('data-swatch'))).toEqual([
      'available',
      'uncovered',
      'committed',
    ])
  })

  // 9,000 asked for, 5,600 available. The gap is stated rather than left as the
  // empty part of a bar.
  test('the uncovered figure is stated, not left to be inferred', () => {
    render(<CoverageBar {...seeded} />)
    expect(screen.getByTestId('coverage-uncovered')).toHaveTextContent('3,400.00 kg')
  })

  test('supply beyond the demand leaves nothing uncovered', () => {
    render(<CoverageBar demandKg={1000} availableKg={5000} committedKg={0} coveragePct={100} />)
    expect(screen.getByTestId('coverage-uncovered')).toHaveTextContent('0.00 kg')
  })

  test('an unknown figure stays unknown rather than becoming a zero', () => {
    render(<CoverageBar demandKg={9000} availableKg={null} committedKg={null} coveragePct={null} />)
    expect(screen.getByTestId('coverage-uncovered')).toHaveTextContent('—')
  })
})

describe('where committed supply is not a figure this level has', () => {
  // The Tower's market tile reports on a village. A row reading "—" there would
  // look like a fault rather than a scope, so the row is simply not drawn.
  test('the committed row is omitted rather than shown empty', () => {
    render(<CoverageBar demandKg={9000} availableKg={5600} coveragePct={62.2} />)

    expect(screen.queryByTestId('coverage-committed')).not.toBeInTheDocument()
    expect(screen.getByTestId('coverage-legend').querySelectorAll('[data-swatch]')).toHaveLength(2)
  })

  test('but the note still says what committed supply means', () => {
    render(<CoverageBar demandKg={9000} availableKg={5600} coveragePct={62.2} />)
    expect(screen.getByText(/promised to a live opportunity/i)).toBeInTheDocument()
  })
})
