import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, test, vi } from 'vitest'
import { createColumnHelper } from '@tanstack/react-table'

import { DataTable } from '@/components/DataTable'
import '@/i18n'

interface Row {
  id: string
  applicant: string
  kw: number | null
}

const col = createColumnHelper<Row>()
const columns = [
  col.accessor('applicant', { header: 'Applicant' }),
  col.accessor('kw', { header: 'Est. kW', cell: (c) => c.getValue() ?? '—' }),
]

const rows: Row[] = [
  { id: 'a', applicant: 'Neema Mwakalinga', kw: 15 },
  { id: 'b', applicant: 'Joseph Kimaro', kw: 3 },
  { id: 'c', applicant: 'Amina Sanga', kw: null },
]

describe('DataTable rendering', () => {
  test('renders headers and a row per record', () => {
    render(<DataTable columns={columns} data={rows} testId="t" rowTestId="row" />)

    expect(screen.getByText('Applicant')).toBeInTheDocument()
    expect(screen.getByText('Est. kW')).toBeInTheDocument()
    expect(screen.getAllByTestId('row')).toHaveLength(3)
  })

  test('a null figure renders as unknown, never as zero', () => {
    render(<DataTable columns={columns} data={rows} testId="t" rowTestId="row" />)
    expect(screen.getByText('—')).toBeInTheDocument()
  })

  // Zero rows is a legitimate answer, so it gets an empty state and never an
  // error or a bare table with no body.
  test('no rows renders the empty state instead of an empty table', () => {
    render(
      <DataTable
        columns={columns}
        data={[]}
        testId="t"
        rowTestId="row"
        empty={{ title: 'No requests match', detail: 'Try a different filter.' }}
      />,
    )

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.getByText('No requests match')).toBeInTheDocument()
    expect(screen.queryByTestId('t')).not.toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})

describe('DataTable row attributes', () => {
  // A guided tour has to open one particular row (a live survey, not whichever
  // is first), and the only way to say so in a selector is an attribute on the
  // row itself.
  test('rowAttrs puts data attributes on each row, computed from that row', () => {
    render(
      <DataTable
        columns={columns}
        data={rows}
        testId="t"
        rowTestId="row"
        rowAttrs={(row) => ({ 'data-owner': row.applicant, 'data-loaded': String(row.kw !== null) })}
      />,
    )

    const [first, , third] = screen.getAllByTestId('row')
    expect(first).toHaveAttribute('data-owner', 'Neema Mwakalinga')
    expect(first).toHaveAttribute('data-loaded', 'true')
    expect(third).toHaveAttribute('data-loaded', 'false')
  })

  test('rowAttrs cannot replace the row test id', () => {
    render(
      <DataTable
        columns={columns}
        data={rows}
        testId="t"
        rowTestId="row"
        rowAttrs={() => ({ 'data-testid': 'hijacked' })}
      />,
    )

    expect(screen.getAllByTestId('row')).toHaveLength(3)
    expect(screen.queryByTestId('hijacked')).not.toBeInTheDocument()
  })
})

describe('DataTable sorting', () => {
  test('a header click sorts, and clicking again reverses it', async () => {
    render(<DataTable columns={columns} data={rows} testId="t" rowTestId="row" />)
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: /applicant/i }))
    let cells = screen.getAllByTestId('row').map((r) => r.textContent)
    expect(cells[0]).toContain('Amina Sanga')

    await user.click(screen.getByRole('button', { name: /applicant/i }))
    cells = screen.getAllByTestId('row').map((r) => r.textContent)
    expect(cells[0]).toContain('Neema Mwakalinga')
  })

  test('sort state is announced for assistive tech', async () => {
    render(<DataTable columns={columns} data={rows} testId="t" rowTestId="row" />)
    const user = userEvent.setup()

    const header = screen.getByRole('columnheader', { name: /applicant/i })
    expect(header).toHaveAttribute('aria-sort', 'none')

    await user.click(screen.getByRole('button', { name: /applicant/i }))
    expect(header).toHaveAttribute('aria-sort', 'ascending')
  })
})

describe('DataTable row activation', () => {
  test('a row reports which record was chosen', async () => {
    const onRowClick = vi.fn()
    render(
      <DataTable columns={columns} data={rows} testId="t" rowTestId="row" onRowClick={onRowClick} />,
    )

    await userEvent.click(screen.getAllByTestId('row')[1])
    expect(onRowClick).toHaveBeenCalledWith(rows[1])
  })

  // A clickable row must be reachable without a mouse.
  test('rows are keyboard activatable when clickable', async () => {
    const onRowClick = vi.fn()
    render(
      <DataTable columns={columns} data={rows} testId="t" rowTestId="row" onRowClick={onRowClick} />,
    )

    const row = screen.getAllByTestId('row')[0]
    expect(row).toHaveAttribute('tabindex', '0')
    row.focus()
    await userEvent.keyboard('{Enter}')
    expect(onRowClick).toHaveBeenCalledWith(rows[0])
  })

  test('rows are not focusable when there is nothing to activate', () => {
    render(<DataTable columns={columns} data={rows} testId="t" rowTestId="row" />)
    expect(screen.getAllByTestId('row')[0]).not.toHaveAttribute('tabindex')
  })
})

/**
 * Five idle ↕ glyphs down a header row were competing with the data for
 * attention, and none of them said anything: a column that is not sorted has
 * no direction to report. The caret now appears only where it means something.
 */
describe('DataTable sort affordance', () => {
  test('no column shows a caret until one is sorted', () => {
    render(<DataTable columns={columns} data={rows} testId="t" rowTestId="row" />)
    expect(screen.queryByText(/[↑↓↕]/)).not.toBeInTheDocument()
  })

  test('the sorted column, and only it, shows a direction', async () => {
    render(<DataTable columns={columns} data={rows} testId="t" rowTestId="row" />)
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: /applicant/i }))

    const carets = screen.getAllByText(/[↑↓]/)
    expect(carets).toHaveLength(1)
    expect(carets[0].textContent).toBe('↑')
    expect(
      screen.getByRole('columnheader', { name: /applicant/i }).contains(carets[0]),
      'the caret belongs to the column it describes',
    ).toBe(true)

    await user.click(screen.getByRole('button', { name: /applicant/i }))
    expect(screen.getAllByText(/[↑↓]/)[0].textContent).toBe('↓')
  })
})

describe('DataTable at a phone width', () => {
  /**
   * QA #8. The wrapper scrolls, the page does not — and `responsive.spec.ts`
   * reaches the wrapper as the table's direct parent, so nothing may be
   * inserted between them.
   */
  test('the scroll wrapper is the table own parent', () => {
    render(<DataTable columns={columns} data={rows} testId="t" rowTestId="row" />)
    const wrapper = screen.getByTestId('t').parentElement
    expect(wrapper?.className).toContain('overflow-x-auto')
  })

  test('a numeric column is right-aligned and tabular', () => {
    render(
      <DataTable
        columns={[
          { accessorKey: 'name', header: 'Applicant' },
          { accessorKey: 'kg', header: 'Quantity', meta: { numeric: true } },
        ]}
        data={[{ name: 'Amina Sanga', kg: '1,200.00' }]}
        testId="t"
        rowTestId="row"
      />,
    )

    const cells = screen.getAllByTestId('row')[0].querySelectorAll('td')
    expect(cells[1].className).toMatch(/text-right/)
    expect(cells[1].className).toMatch(/tabular/)
    expect(cells[0].className).not.toMatch(/text-right/)
  })
})
