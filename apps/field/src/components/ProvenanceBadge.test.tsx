import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'

import { ProvenanceBadge } from '@/components/ProvenanceBadge'
import '@/i18n'

const AT = '2026-09-09T21:30:00Z'

describe('ProvenanceBadge source treatments', () => {
  // "The five source values get five distinct treatments." Distinctness is
  // asserted structurally so a redesign cannot quietly collapse two of them.
  const sources = [
    ['farmer_reported', 'Farmer reported'],
    ['field_verified', 'Field verified'],
    ['transaction_derived', 'From a transaction'],
    ['sensor_derived', 'Sensor measured'],
    ['model_estimated', 'Estimated'],
  ] as const

  for (const [source, label] of sources) {
    test(`${source} names itself as "${label}"`, () => {
      render(<ProvenanceBadge source={source} verification="unverified" capturedAt={AT} />)
      expect(screen.getByTestId('provenance-badge')).toHaveTextContent(label)
    })
  }

  test('each source carries its own data-source, all five distinct', () => {
    const seen = new Set<string>()
    for (const [source] of sources) {
      const { unmount } = render(
        <ProvenanceBadge source={source} verification="unverified" capturedAt={AT} />,
      )
      const attr = screen.getByTestId('provenance-badge').getAttribute('data-source')
      expect(attr).toBe(source)
      seen.add(attr!)
      unmount()
    }
    expect(seen.size).toBe(5)
  })

  // An estimate is always labelled an estimate; a model figure must never read
  // as a measurement.
  test('an estimated figure never claims to be measured', () => {
    render(<ProvenanceBadge source="model_estimated" verification="unverified" capturedAt={AT} />)
    const badge = screen.getByTestId('provenance-badge')
    expect(badge).toHaveTextContent('Estimated')
    expect(badge).not.toHaveTextContent(/measured/i)
  })
})

describe('ProvenanceBadge verification state', () => {
  const statuses = [
    ['unverified', 'Unverified'],
    ['pending', 'Pending'],
    ['verified', 'Verified'],
    ['disputed', 'Disputed'],
  ] as const

  for (const [verification, label] of statuses) {
    test(`${verification} is shown as "${label}"`, () => {
      render(
        <ProvenanceBadge source="farmer_reported" verification={verification} capturedAt={AT} />,
      )
      expect(screen.getByTestId('provenance-verification')).toHaveTextContent(label)
    })
  }

  test('verification is exposed separately from source', () => {
    render(<ProvenanceBadge source="farmer_reported" verification="verified" capturedAt={AT} />)
    expect(screen.getByTestId('provenance-badge')).toHaveAttribute('data-verification', 'verified')
  })
})

describe('ProvenanceBadge optional detail', () => {
  test('confidence is shown when present', () => {
    render(
      <ProvenanceBadge
        source="farmer_reported"
        verification="unverified"
        confidence="low"
        capturedAt={AT}
      />,
    )
    expect(screen.getByTestId('provenance-confidence')).toHaveTextContent('Low')
  })

  test('confidence is omitted entirely when absent, not shown as blank', () => {
    render(<ProvenanceBadge source="farmer_reported" verification="unverified" capturedAt={AT} />)
    expect(screen.queryByTestId('provenance-confidence')).not.toBeInTheDocument()
  })

  test('the long form is available as a tooltip, in the project timezone', () => {
    render(
      <ProvenanceBadge
        source="field_verified"
        verification="verified"
        confidence="high"
        capturedAt={AT}
        capturedBy="Salima Officer"
      />,
    )
    const title = screen.getByTestId('provenance-badge').getAttribute('title') ?? ''
    expect(title).toContain('Field verified')
    expect(title).toContain('Verified')
    expect(title).toContain('Salima Officer')
    // 21:30 UTC is the next day in Africa/Dar_es_Salaam.
    expect(title).toContain('10 Sep 2026')
  })

  test('an unknown capturer is omitted rather than rendered as undefined', () => {
    render(<ProvenanceBadge source="field_verified" verification="verified" capturedAt={AT} />)
    const title = screen.getByTestId('provenance-badge').getAttribute('title') ?? ''
    expect(title).not.toMatch(/undefined|null/)
  })

  test('a missing capturedAt degrades to unknown rather than an invalid date', () => {
    render(<ProvenanceBadge source="field_verified" verification="verified" capturedAt={null} />)
    const title = screen.getByTestId('provenance-badge').getAttribute('title') ?? ''
    expect(title).not.toMatch(/Invalid Date/)
    expect(title).toContain('—')
  })
})
