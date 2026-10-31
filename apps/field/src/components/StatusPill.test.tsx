import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'

import { StatusPill } from '@/components/StatusPill'
import '@/i18n'

describe('StatusPill for a PUE request', () => {
  const statuses = [
    ['draft', 'Draft'],
    ['submitted', 'Submitted'],
    ['under_review', 'Under review'],
    ['approved', 'Approved'],
    ['rejected', 'Rejected'],
    ['withdrawn', 'Withdrawn'],
  ] as const

  for (const [status, label] of statuses) {
    test(`${status} reads as "${label}"`, () => {
      render(<StatusPill kind="request" status={status} />)
      expect(screen.getByTestId('status-pill')).toHaveTextContent(label)
    })
  }

  test('the raw status is exposed for styling and assertions', () => {
    render(<StatusPill kind="request" status="under_review" />)
    expect(screen.getByTestId('status-pill')).toHaveAttribute('data-status', 'under_review')
  })

  // Distinctness is asserted on the whole visual signature rather than on the
  // class attribute alone: the redesign carries the tints as
  // inline style, and a pill that stopped being distinguishable would be just
  // as wrong however it was written.
  test('all six request statuses are visually distinct', () => {
    const seen = new Set<string>()
    for (const [status] of statuses) {
      const { unmount } = render(<StatusPill kind="request" status={status} />)
      seen.add(signature(screen.getByTestId('status-pill')))
      unmount()
    }
    expect(seen.size).toBe(6)
  })

  // The only solid fill in the system. An approved request is a decision with
  // capacity consequences; nothing else in the product may look like one.
  test('approved is the only filled request pill', () => {
    const filled: string[] = []
    for (const [status] of statuses) {
      const { unmount } = render(<StatusPill kind="request" status={status} />)
      if (isFilled(screen.getByTestId('status-pill'))) filled.push(status)
      unmount()
    }
    expect(filled).toEqual(['approved'])
  })
})

describe('StatusPill for surveys and vouchers', () => {
  const surveys = [
    ['draft', 'Draft'],
    ['live', 'Live'],
    ['closed', 'Closed'],
  ] as const
  for (const [status, label] of surveys) {
    test(`survey ${status} reads as "${label}"`, () => {
      render(<StatusPill kind="survey" status={status} />)
      expect(screen.getByTestId('status-pill')).toHaveTextContent(label)
    })
  }

  // "expired" is derived from the expiry date; it is not a stored status.
  const vouchers = [
    ['issued', 'Not yet collected'],
    ['redeemed', 'Collected'],
    ['void', 'Cancelled'],
    ['expired', 'Expired'],
  ] as const
  for (const [status, label] of vouchers) {
    test(`voucher ${status} reads as "${label}"`, () => {
      render(<StatusPill kind="voucher" status={status} />)
      expect(screen.getByTestId('status-pill')).toHaveTextContent(label)
    })
  }

  test('all four voucher states are visually distinct', () => {
    const seen = new Set<string>()
    for (const [status] of vouchers) {
      const { unmount } = render(<StatusPill kind="voucher" status={status} />)
      seen.add(signature(screen.getByTestId('status-pill')))
      unmount()
    }
    expect(seen.size).toBe(4)
  })

  // A collected incentive is not an approval decision, so it is not the fill.
  test('no voucher or survey pill uses the approval fill', () => {
    for (const [status] of vouchers) {
      const { unmount } = render(<StatusPill kind="voucher" status={status} />)
      expect(isFilled(screen.getByTestId('status-pill'))).toBe(false)
      unmount()
    }
    for (const [status] of surveys) {
      const { unmount } = render(<StatusPill kind="survey" status={status} />)
      expect(isFilled(screen.getByTestId('status-pill'))).toBe(false)
      unmount()
    }
  })
})

/** Class list plus inline style — everything that decides how a pill looks. */
function signature(element: HTMLElement): string {
  return `${element.className}|${element.getAttribute('style') ?? ''}`
}

/** A solid brand fill, as opposed to a tint or bare paper. */
function isFilled(element: HTMLElement): boolean {
  const style = element.getAttribute('style') ?? ''
  return /background:\s*var\(--(accent|green|primary|flag-ink|green-ink)\)/.test(style)
}

describe('StatusPill for demand and opportunity', () => {
  test('demand statuses render', () => {
    render(<StatusPill kind="demand" status="open" />)
    expect(screen.getByTestId('status-pill')).toHaveTextContent('Open')
  })

  // An opportunity is not a sale. 'accepted' means both sides agreed to talk
  // further — nothing has moved — so the label must not imply a transaction.
  test('an accepted opportunity does not read as a sale', () => {
    render(<StatusPill kind="opportunity" status="accepted" />)
    const pill = screen.getByTestId('status-pill')
    expect(pill).toHaveTextContent('Accepted')
    expect(pill).not.toHaveTextContent(/sold|sale|paid|delivered/i)
  })

  // An opportunity's accepted means both sides agreed to keep talking. A
  // request's approved is a decision. The old palette gave them the same
  // treatment; every opportunity pill is now outlined on white, and accepted is
  // distinguished by a heavier edge rather than by a fill.
  test('an accepted opportunity never looks like an approved request', () => {
    const { unmount } = render(<StatusPill kind="request" status="approved" />)
    const approved = signature(screen.getByTestId('status-pill'))
    unmount()

    render(<StatusPill kind="opportunity" status="accepted" />)
    const accepted = screen.getByTestId('status-pill')

    expect(signature(accepted)).not.toBe(approved)
    expect(isFilled(accepted), 'an opportunity is not a sale, and never a fill').toBe(false)
  })

  test('every opportunity state is outlined on white', () => {
    for (const status of ['proposed', 'shared', 'accepted', 'declined', 'lapsed'] as const) {
      const { unmount } = render(<StatusPill kind="opportunity" status={status} />)
      expect(isFilled(screen.getByTestId('status-pill')), status).toBe(false)
      unmount()
    }
  })

  test('the three kinds do not share a translation namespace', () => {
    const { unmount } = render(<StatusPill kind="request" status="approved" />)
    const request = screen.getByTestId('status-pill').textContent
    unmount()
    render(<StatusPill kind="opportunity" status="proposed" />)
    expect(screen.getByTestId('status-pill').textContent).not.toBe(request)
  })
})
