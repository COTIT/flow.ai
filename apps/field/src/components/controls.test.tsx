import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'

import { Card, ControlLabel, IndicativePill, Loading, ProductNote, TableCard } from '@/components/controls'
import '@/i18n'

describe('shared controls', () => {
  test('TableCard uses the shared white table surface', () => {
    render(<TableCard><div data-testid="rows">Rows</div></TableCard>)
    expect(screen.getByTestId('rows').parentElement?.parentElement).toHaveClass('bg-paper')
  })

  test('existing control helpers keep their semantic content', () => {
    render(
      <>
        <ControlLabel label="Village"><input aria-label="Village" /></ControlLabel>
        <Card><span>Card content</span></Card>
        <IndicativePill />
        <ProductNote>Opportunity note</ProductNote>
        <Loading testId="loading" />
      </>,
    )

    expect(screen.getByText('Village')).toBeInTheDocument()
    expect(screen.getByText('Card content')).toBeInTheDocument()
    expect(screen.getByText('Indicative')).toBeInTheDocument()
    expect(screen.getByText('Opportunity note')).toBeInTheDocument()
    expect(screen.getByTestId('loading')).toBeInTheDocument()
  })
})
