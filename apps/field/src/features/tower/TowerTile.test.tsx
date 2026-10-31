import { render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, ...rest }: { children: React.ReactNode }) => <a href="#x" {...rest}>{children}</a>,
}))

const { TowerTile, Figure } = await import('@/features/tower/TowerTile')
await import('@/i18n')

/**
 * Spec 8.2: a number that cannot be traced does not belong on this screen, so
 * every tile carrying a figure carries a way to the rows behind it.
 */
describe('TowerTile', () => {
  test('a tile with a drill target offers the way to its rows', () => {
    render(
      <TowerTile id="production" title="Production" drillTo="/ops/tower/production">
        <Figure label="Expected" value="12,000.00 kg" testId="expected" />
      </TowerTile>,
    )

    expect(screen.getByTestId('tile-production')).toBeInTheDocument()
    expect(screen.getByTestId('tile-drill')).toBeInTheDocument()
  })

  // A guided tour points at a tile by test id, and its checker can only read an
  // id that is written out in source. The tile still derives one when not told.
  test('a tile takes the test id it is given, and derives one otherwise', () => {
    render(
      <>
        <TowerTile id="production" testId="tile-given" title="Production">
          <p>one</p>
        </TowerTile>
        <TowerTile id="energy" title="Energy">
          <p>two</p>
        </TowerTile>
      </>,
    )

    expect(screen.getByTestId('tile-given')).toHaveTextContent('one')
    expect(screen.queryByTestId('tile-production')).not.toBeInTheDocument()
    expect(screen.getByTestId('tile-energy')).toHaveTextContent('two')
  })

  test('a tile with nothing behind it offers no link that goes nowhere', () => {
    render(
      <TowerTile id="production" title="Production">
        <p>nothing yet</p>
      </TowerTile>,
    )

    expect(screen.queryByTestId('tile-drill')).not.toBeInTheDocument()
  })

  /**
   * QA #29. Each tile showed its own "Loading…" while the header's "See the
   * records" link rendered immediately and was clickable — so a click before
   * the headline resolved landed on a drill-down whose own query had not
   * started, from a figure nobody had seen yet.
   */
  test('while the figure is still loading the drill is not clickable', () => {
    render(
      <TowerTile id="production" title="Production" drillTo="/ops/tower/production" loading>
        <p>Loading…</p>
      </TowerTile>,
    )

    expect(screen.queryByRole('link')).not.toBeInTheDocument()
  })

  // Removing the label entirely would make the header jump when the figure
  // arrives. It stays, plainly, as text.
  test('the label stays in place, so the header does not jump', () => {
    render(
      <TowerTile id="production" title="Production" drillTo="/ops/tower/production" loading>
        <p>Loading…</p>
      </TowerTile>,
    )

    expect(screen.getByTestId('tile-drill')).toBeInTheDocument()
    expect(screen.getByTestId('tile-drill').tagName).not.toBe('A')
  })

  test('and it becomes a link once the figure is there', () => {
    render(
      <TowerTile id="production" title="Production" drillTo="/ops/tower/production" loading={false}>
        <Figure label="Expected" value="12,000.00 kg" />
      </TowerTile>,
    )

    expect(screen.getByTestId('tile-drill').tagName).toBe('A')
  })

  // An unloaded tile is not an error and not an empty state.
  test('loading is announced to assistive technology as busy', () => {
    render(
      <TowerTile id="production" title="Production" drillTo="/x" loading>
        <p>Loading…</p>
      </TowerTile>,
    )

    expect(screen.getByTestId('tile-production')).toHaveAttribute('aria-busy', 'true')
  })
})
