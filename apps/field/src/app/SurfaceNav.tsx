import { useLayoutEffect, useRef, type ComponentType } from 'react'
import {
  BookOpen,
  Briefcase,
  CircleCheckBig,
  FileText,
  Gauge,
  Handshake,
  MapPin,
  Package,
  Sprout,
  UserPlus,
  Users,
  Wrench,
  ClipboardList,
  ClipboardCheck,
  ScanLine,
} from 'lucide-react'
import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import type { NavBadgeSource, NavItem, NavLayout } from '@/app/nav'
import { useSurveyBadgeCount } from '@/features/farmer/useSurveyEligibility'

// navItemsFor builds paths from the route map as plain strings. The router's
// `to` is a typed union, and this is the single place the two meet.
type LinkTo = Parameters<typeof Link>[0]['to']

/**
 * Icon plus label, never icon alone. The label carries the meaning for a
 * first-time user and survives translation into a language none of these
 * glyphs were drawn for; the icon carries recognition on the tenth visit.
 *
 * Every one of them is `aria-hidden`: the label is already the accessible
 * name, and a second one would only be read twice.
 */
const ICON: Record<string, ComponentType<{ size?: number; strokeWidth?: number }>> = {
  '/farm/my-farm': Sprout,
  '/farm/equipment': Wrench,
  '/farm/requests': FileText,
  '/farm/opportunities': Handshake,
  '/farm/surveys': ClipboardCheck,
  '/officer/register': UserPlus,
  '/officer/people': Users,
  '/officer/verify': CircleCheckBig,
  '/officer/redeem': ScanLine,
  '/ops/requests': ClipboardList,
  '/ops/demand': Package,
  '/ops/catalogue': BookOpen,
  '/ops/buyers': Briefcase,
  '/ops/villages': MapPin,
  '/ops/surveys': ClipboardCheck,
  '/ops/tower': Gauge,
}

/**
 * Spec 4.1: farmer and officer get a bottom tab bar (mobile-first), ops gets a
 * sidebar (desktop-first).
 *
 * Desktop-first is the spec's choice for ops and stays the choice. But below
 * `lg` the fixed 13rem sidebar squeezed the content column until figures were
 * cut mid-number — "12,000" rendering as "12,0" — without the page scrolling
 * sideways to reveal them (QA #8). Silently truncated data is not the same
 * thing as desktop-first, so the sidebar becomes a horizontal strip there
 * instead of disappearing: collapsing is not hiding, and every destination
 * stays reachable.
 *
 * Touch targets are `min-height`, never `height`: 44px on ops, 60px on a field
 * surface, and a Kiswahili label that runs 30% longer has to be allowed to
 * wrap rather than be clipped.
 */
export function SurfaceNav({ layout, items }: { layout: NavLayout; items: NavItem[] }) {
  const { t } = useTranslation()
  if (layout === 'none' || items.length === 0) return null

  if (layout === 'sidebar') {
    return (
      <nav
        aria-label={t('a11y.primaryNav')}
        data-testid="nav-sidebar"
        className="w-full shrink-0 p-3 lg:w-[216px] lg:border-b-0 lg:border-r"
        style={{ background: 'var(--paper)', borderBottom: '1px solid var(--rule)' }}
      >
        <ul className="flex gap-1 overflow-x-auto lg:block lg:space-y-0.5 lg:overflow-visible">
          {items.map((item) => (
            <li key={item.to} className="shrink-0">
              <SurfaceLink item={item} layout="sidebar" label={t(item.labelKey)} />
            </li>
          ))}
        </ul>
      </nav>
    )
  }

  return <TabBar items={items} />
}

/**
 * The bottom bar, and the one number anything else pinned to the bottom of the
 * viewport needs.
 *
 * The bar is `position: fixed`, so flowing content needs bottom padding to
 * remain clear of it. `main` owns that padding; Register's submit bar stays in
 * normal flow so it cannot crop the final confidence option on a phone.
 *
 * Measured, not written down: a tab is `min-height: 60px` and renders at 63
 * with an icon above a label, and a Kiswahili label is allowed to wrap and
 * make it taller again. A constant would be a guess that drifts in silence,
 * which is how this broke the first time. Every label is held to one line on
 * a 375px phone by `e2e/shell.spec.ts` (measured in the real font, both
 * languages), so wrapping stays allowed but is not expected.
 */
function TabBar({ items }: { items: NavItem[] }) {
  const { t } = useTranslation()
  const bar = useRef<HTMLElement>(null)

  useLayoutEffect(() => {
    const publish = () => {
      if (!bar.current) return
      const height = bar.current.getBoundingClientRect().height
      document.documentElement.style.setProperty('--tab-bar-height', `${height}px`)
    }

    publish()
    window.addEventListener('resize', publish)
    // The safe-area offset changes without a window resize on some devices, and
    // the bar's own box is what carries it. jsdom has no ResizeObserver.
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(publish)
    if (bar.current) observer?.observe(bar.current)
    return () => {
      window.removeEventListener('resize', publish)
      observer?.disconnect()
      document.documentElement.style.removeProperty('--tab-bar-height')
    }
    // The labels are the height: a language change re-renders and re-measures.
  }, [items, t])

  return (
    <nav
      ref={bar}
      aria-label={t('a11y.primaryNav')}
      data-testid="nav-tabs"
      className="pointer-events-none fixed inset-x-0 bottom-0 z-10 box-border w-full max-w-full overflow-hidden pb-[var(--tab-bar-bottom)] pl-[max(12px,env(safe-area-inset-left))] pr-[max(12px,env(safe-area-inset-right))]"
    >
      <ul className="pointer-events-auto flex w-full min-w-0 overflow-hidden rounded-[var(--radius-frame)] border border-rule-2 bg-paper p-[3px] gap-0.5">
        {items.map((item) => (
          <li key={item.to} className="min-w-0 flex-1">
            <SurfaceLink item={item} layout="tabs" label={t(item.labelKey)} />
          </li>
        ))}
      </ul>
    </nav>
  )
}

function SurfaceLink({
  item,
  layout,
  label,
}: {
  item: NavItem
  layout: 'sidebar' | 'tabs'
  label: string
}) {
  const Icon = ICON[item.to]
  const sidebar = layout === 'sidebar'

  return (
    <Link
      to={item.to as LinkTo}
      // The active treatment is a tinted ground plus a rule — a border, not the
      // inset box-shadow the design file draws it with, because nothing in this
      // build composites a shadow layer.
      className={
        sidebar
          ? 'flex items-center gap-[11px] rounded-[var(--radius-control)] px-3 py-2.5 text-ink-2 hover:bg-sand-2 data-[status=active]:border-l-[3px] data-[status=active]:border-l-primary data-[status=active]:bg-primary-tint data-[status=active]:pl-[9px] data-[status=active]:font-semibold data-[status=active]:text-primary-ink'
          : 'relative flex w-full min-w-0 flex-col items-center justify-center gap-[5px] break-words px-1 py-2 text-center text-wrap-balance text-ink-2 rounded-[var(--radius-card)] data-[status=active]:bg-primary-tint data-[status=active]:font-semibold data-[status=active]:text-primary-ink'
      }
      style={sidebar ? { minHeight: 44, fontSize: 15 } : { minHeight: 60, fontSize: 12 }}
    >
      {Icon && <Icon aria-hidden size={sidebar ? 18 : 22} strokeWidth={2} />}
      {label}
      {item.badge && <NavBadge source={item.badge} layout={layout} />}
    </Link>
  )
}

/** Each source is its own component, so each calls exactly one hook. */
const BADGE_COUNT: Record<NavBadgeSource, () => number> = {
  surveys: useSurveyBadgeCount,
}

/**
 * A count on a nav item — the in-app notice that something new is waiting.
 * Zero renders nothing: an empty badge is noise, not information.
 */
function NavBadge({ source, layout }: { source: NavBadgeSource; layout: 'sidebar' | 'tabs' }) {
  const { t } = useTranslation()
  const count = BADGE_COUNT[source]()
  if (count === 0) return null
  const tabs = layout === 'tabs'
  return (
    <span
      data-testid={`nav-badge-${source}`}
      // On a tab the badge is out of flow, pinned to the icon's top-right
      // corner: in flow it was a third row that grew this tab and pushed every
      // other tab's centred content up. The icon is 22px and centred, so its
      // right edge is 50% + 11px. The paper ring separates it from the glyph.
      className={
        tabs
          ? 'type-note pointer-events-none absolute top-[3px] left-[calc(50%+2px)] inline-flex h-[18px] min-w-[18px] items-center justify-center px-1 tabular-nums'
          : 'type-note ml-auto inline-flex min-w-5 items-center justify-center px-1.5 tabular-nums'
      }
      style={{
        borderRadius: 'var(--radius-pill)',
        background: 'var(--flag-ink)',
        color: 'var(--paper)',
        fontWeight: 600,
        lineHeight: tabs ? '18px' : '20px',
        ...(tabs ? { boxShadow: '0 0 0 2px var(--paper)' } : {}),
      }}
    >
      <span aria-hidden>{count > 9 ? '9+' : count}</span>
      <span className="sr-only">{t('surveys.badge', { count })}</span>
    </span>
  )
}
