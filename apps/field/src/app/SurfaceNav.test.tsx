import { render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

vi.mock('@tanstack/react-router', () => ({
  Link: ({
    children,
    to,
    ...rest
  }: { children: React.ReactNode; to: string } & Record<string, unknown>) => (
    <a href={to} {...rest}>
      {children}
    </a>
  ),
}))

const surveyBadge = vi.fn(() => 0)
vi.mock('@/features/farmer/useSurveyEligibility', () => ({
  useSurveyBadgeCount: () => surveyBadge(),
}))

const { SurfaceNav } = await import('@/app/SurfaceNav')
const { navItemsFor } = await import('@/app/nav')
await import('@/i18n')

const OPS = navItemsFor('ops', [{ role: 'ops', revoked_at: null }] as never)
const FARMER = navItemsFor('farmer', [{ role: 'farmer', revoked_at: null }] as never)

/**
 * Icon plus label, never icon alone. The label carries the meaning for a
 * first-time user and survives translation; the icon carries recognition on the
 * tenth visit. Which also means the icon must contribute nothing to the text —
 * `shell.spec.ts` asserts the tab labels as an exact array, and an icon that
 * leaked a text node would break it for a reason unrelated to navigation.
 */
describe('SurfaceNav marks its destinations', () => {
  test('every ops sidebar item carries an icon beside its label', () => {
    render(<SurfaceNav layout="sidebar" items={OPS} />)
    const links = screen.getByTestId('nav-sidebar').querySelectorAll('a')

    expect(links).toHaveLength(OPS.length)
    for (const link of links) {
      expect(link.querySelector('svg'), link.textContent ?? '').not.toBeNull()
    }
  })

  test('every field tab carries an icon above its label', () => {
    render(<SurfaceNav layout="tabs" items={FARMER} />)
    const links = screen.getByTestId('nav-tabs').querySelectorAll('a')

    expect(links).toHaveLength(FARMER.length)
    for (const link of links) {
      expect(link.querySelector('svg'), link.textContent ?? '').not.toBeNull()
    }
  })

  test('the icons add no text of their own', () => {
    render(<SurfaceNav layout="tabs" items={FARMER} />)
    const labels = [...screen.getByTestId('nav-tabs').querySelectorAll('a')].map(
      (link) => link.textContent,
    )

    expect(labels).toEqual(['My farm', 'Resources', 'Requests', 'Opportunities', 'Surveys'])
  })

  test('every icon is hidden from assistive tech, since the label already says it', () => {
    render(<SurfaceNav layout="sidebar" items={OPS} />)
    for (const svg of screen.getByTestId('nav-sidebar').querySelectorAll('svg')) {
      expect(svg.getAttribute('aria-hidden')).toBe('true')
    }
  })
})

describe('SurfaceNav touch targets', () => {
  // 44px on ops, 60px on a field surface, and `min-height` rather than
  // `height`: a Kiswahili label runs longer and has to be allowed to wrap.
  test('a field tab is 60px and does not fix its height', () => {
    render(<SurfaceNav layout="tabs" items={FARMER} />)
    const tabs = screen.getByTestId('nav-tabs')
    const style = tabs.querySelector('a')?.getAttribute('style') ?? ''

    expect(style).toMatch(/min-height:\s*60px/)
    expect(style).not.toMatch(/(^|;)\s*height:/)
    expect(tabs).toHaveClass('w-full', 'max-w-full', 'overflow-hidden')
    expect(tabs.querySelector('ul')).toHaveClass('w-full', 'min-w-0')
    expect(tabs.querySelector('li')).toHaveClass('min-w-0', 'flex-1')
    expect(tabs.querySelector('a')).toHaveClass('w-full', 'min-w-0')
  })

  // The bar floats: a transparent, click-through strip owns the safe-area
  // offset, and the pill inside it is the only thing that paints or takes a tap.
  test('the strip is click-through and offsets itself from the system gesture area', () => {
    render(<SurfaceNav layout="tabs" items={FARMER} />)
    const tabs = screen.getByTestId('nav-tabs')

    expect(tabs).toHaveClass('pointer-events-none', 'fixed', 'inset-x-0', 'bottom-0')
    expect(tabs.className).toContain('pb-[var(--tab-bar-bottom)]')
    expect(tabs.className).toContain('pl-[max(12px,env(safe-area-inset-left))]')
    expect(tabs.className).toContain('pr-[max(12px,env(safe-area-inset-right))]')
    expect(tabs.className, 'the strip paints nothing').not.toMatch(/(^|\s)(bg-|border(\s|$))/)
  })

  test('the pill paints a bordered paper surface and takes the taps', () => {
    render(<SurfaceNav layout="tabs" items={FARMER} />)
    const pill = screen.getByTestId('nav-tabs').querySelector('ul')!

    expect(pill).toHaveClass('pointer-events-auto', 'bg-paper', 'border', 'border-rule-2')
    expect(pill.className).toContain('rounded-[var(--radius-frame)]')
  })

  // A top rule is a straight line across a rounded end; the active tab is a
  // tinted fill instead.
  test('the active tab is a tinted fill, not a top rule', () => {
    render(<SurfaceNav layout="tabs" items={FARMER} />)
    const link = screen.getByTestId('nav-tabs').querySelector('a')!

    expect(link.className).toContain('data-[status=active]:bg-primary-tint')
    expect(link.className).not.toContain('border-t-2')
  })

  test('an ops item is 44px and does not fix its height', () => {
    render(<SurfaceNav layout="sidebar" items={OPS} />)
    const style = screen.getByTestId('nav-sidebar').querySelector('a')?.getAttribute('style') ?? ''

    expect(style).toMatch(/min-height:\s*44px/)
    expect(style).not.toMatch(/(^|;)\s*height:/)
  })
})

/**
 * The bar is `position: fixed`, so `main` needs to reserve space above it for
 * a mobile form footer and the final fields before that footer.
 *
 * The height is measured rather than written down. A tab is `min-height: 60px`
 * and the real one renders at 63 with an icon above a label, and a Kiswahili
 * label is allowed to wrap and make it taller still — so a constant would be a
 * guess that drifts silently, which is exactly how this broke the first time.
 */
describe('SurfaceNav publishes the bar height', () => {
  const read = () => document.documentElement.style.getPropertyValue('--tab-bar-height')

  test('a field surface sets it in pixels', () => {
    render(<SurfaceNav layout="tabs" items={FARMER} />)
    expect(read()).toMatch(/^\d+(\.\d+)?px$/)
  })

  test('and clears it on unmount, so the ops surface is not padded for a bar it has not got', () => {
    const view = render(<SurfaceNav layout="tabs" items={FARMER} />)
    expect(read()).not.toBe('')
    view.unmount()
    expect(read()).toBe('')
  })

  test('the ops sidebar never sets it', () => {
    document.documentElement.style.removeProperty('--tab-bar-height')
    render(<SurfaceNav layout="sidebar" items={OPS} />)
    expect(read()).toBe('')
  })
})

/** The in-app notice that a survey is waiting: the MVP sends no SMS or push. */
describe('the Surveys tab badge', () => {
  test('shows how many surveys the household may answer now', () => {
    surveyBadge.mockReturnValue(2)
    render(<SurfaceNav items={FARMER} layout="tabs" />)
    const badge = screen.getByTestId('nav-badge-surveys')
    expect(badge).toHaveTextContent('2')
    expect(badge).toHaveTextContent('2 new surveys')
  })

  // The badge must never change the bar's geometry. As a third flex-column
  // child it added a row, grew the Surveys tab and pushed every other tab's
  // centred content upward. Out of flow, it cannot. jsdom has no layout, so
  // what is asserted is the mechanism; the measured heights are checked in a
  // real browser.
  test('floats over the icon corner instead of adding a row to the tab', () => {
    surveyBadge.mockReturnValue(1)
    render(<SurfaceNav items={FARMER} layout="tabs" />)
    const badge = screen.getByTestId('nav-badge-surveys')

    expect(badge).toHaveClass('absolute')
    expect(badge.closest('a')).toHaveClass('relative')
  })

  test('caps the visible count at 9+ and keeps the real number for a screen reader', () => {
    surveyBadge.mockReturnValue(12)
    render(<SurfaceNav items={FARMER} layout="tabs" />)
    const badge = screen.getByTestId('nav-badge-surveys')

    expect(badge.querySelector('[aria-hidden]')).toHaveTextContent('9+')
    expect(badge).toHaveTextContent('12 new surveys')
  })

  test('shows nothing when there is nothing to answer', () => {
    surveyBadge.mockReturnValue(0)
    render(<SurfaceNav items={FARMER} layout="tabs" />)
    expect(screen.queryByTestId('nav-badge-surveys')).not.toBeInTheDocument()
  })
})
