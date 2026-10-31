import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'

import { TableSurface } from '@/components/TableSurface'

describe('TableSurface', () => {
  test('keeps the table and its toolbar inside one white bordered surface', () => {
    render(
      <TableSurface toolbar={<div data-testid="filters">Filters</div>}>
        <div data-testid="table-content">Rows</div>
      </TableSurface>,
    )

    const surface = screen.getByTestId('table-content').parentElement!
    expect(surface.className).toContain('bg-paper')
    expect(surface.className).toContain('overflow-hidden')
    expect(screen.getByTestId('filters').parentElement).toHaveClass('bg-paper')
    expect(screen.getByTestId('table-content')).toHaveTextContent('Rows')
  })

  test('accepts responsive composition and caller classes', () => {
    render(
      <TableSurface className="max-w-screen-xl">
        <div className="overflow-x-auto">Wide table</div>
      </TableSurface>,
    )

    expect(screen.getByText('Wide table')).toHaveClass('overflow-x-auto')
    expect(screen.getByText('Wide table').parentElement).toHaveClass('max-w-screen-xl')
  })
})
