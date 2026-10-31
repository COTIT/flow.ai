import { useState } from 'react'
import {
  flexRender,
  getCoreRowModel,
  getSortedRowModel,
  useReactTable,
  type ColumnDef,
  type RowData,
  type SortingState,
} from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'

export interface DataTableProps<T> {
  // ColumnDef's value generic is invariant in v8, so a wrapper cannot narrow
  // it without rejecting every real column definition. This is the signature
  // TanStack's own examples use.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  columns: ColumnDef<T, any>[]
  data: T[]
  testId: string
  rowTestId: string
  onRowClick?: (row: T) => void
  /**
   * Extra attributes for a row, computed from it — `data-status` and the like.
   * They exist so a selector can say WHICH row (a guided tour opening the live
   * survey, not whichever is first); the row's own test id always wins.
   */
  rowAttrs?: (row: T) => Record<string, string>
  empty?: { title: string; detail?: string }
}

/**
 * Per-column presentation. A figure is right-aligned and tabular so the digits
 * of one row stack on the digits of the next; a name is not. Unit marks belong
 * in the header, once, rather than in every cell.
 */
declare module '@tanstack/react-table' {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface ColumnMeta<TData extends RowData, TValue> {
    numeric?: boolean
  }
}

/**
 * TanStack Table wrapper — spec §9.3. Sorting, an empty state, and rows that
 * lead somewhere.
 *
 * Filter state is deliberately NOT held here: spec 7.2 puts it in the URL as
 * validated search params, so the owning screen holds it and passes filtered
 * data down. A table that owned its own filters would make that state
 * unshareable and unreloadable.
 *
 * The head is sunken sand with 11px uppercase labels, rows are one hairline
 * apart with no zebra, and the sort caret appears only on the column that is
 * actually sorted — five idle ↕ glyphs were competing with the data and saying
 * nothing. The `overflow-x-auto` wrapper is the table's own parent and stays
 * that way: at 375px the table scrolls inside itself and the page does not
 * (QA #8), and `responsive.spec.ts` reaches the wrapper through that
 * relationship.
 */
export function DataTable<T>({
  columns,
  data,
  testId,
  rowTestId,
  onRowClick,
  rowAttrs,
  empty,
}: DataTableProps<T>) {
  const { t } = useTranslation()
  const [sorting, setSorting] = useState<SortingState>([])

  // react-hooks/incompatible-library does not model TanStack Table's instance,
  // which is created per render and holds no React state of its own — sorting
  // lives in the useState above.
  // eslint-disable-next-line react-hooks/incompatible-library
  const table = useReactTable({
    data,
    columns,
    state: { sorting },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  })

  // Zero rows is an answer, not a failure. Rendering a headed table with no
  // body reads as something broken.
  if (data.length === 0) {
    return (
      <EmptyState
        title={empty?.title ?? t('table.emptyTitle')}
        detail={empty?.detail ?? t('table.emptyDetail')}
      />
    )
  }

  const clickable = Boolean(onRowClick)

  return (
    <div className="relative overflow-x-auto">
      <table
        data-testid={testId}
        className="w-full border-collapse"
        style={{ fontSize: 15, lineHeight: '22px' }}
      >
        <thead>
          {table.getHeaderGroups().map((group) => (
            <tr key={group.id} className="text-left" style={{ background: 'var(--sand-2)' }}>
              {group.headers.map((header) => {
                const sorted = header.column.getIsSorted()
                const numeric = header.column.columnDef.meta?.numeric === true
                return (
                  <th
                    key={header.id}
                    scope="col"
                    aria-sort={
                      sorted === 'asc' ? 'ascending' : sorted === 'desc' ? 'descending' : 'none'
                    }
                    className={
                      numeric
                        ? 'type-column-label px-4 py-[11px] text-right'
                        : 'type-column-label px-4 py-[11px]'
                    }
                    style={{ color: 'var(--ink-3)' }}
                  >
                    {header.column.getCanSort() ? (
                      <button
                        type="button"
                        onClick={header.column.getToggleSortingHandler()}
                        className="inline-flex items-center gap-[5px]"
                      >
                        {flexRender(header.column.columnDef.header, header.getContext())}
                        {sorted && (
                          <span aria-hidden style={{ color: 'var(--primary-ink)' }}>
                            {sorted === 'asc' ? '↑' : '↓'}
                          </span>
                        )}
                      </button>
                    ) : (
                      flexRender(header.column.columnDef.header, header.getContext())
                    )}
                  </th>
                )
              })}
            </tr>
          ))}
        </thead>

        <tbody>
          {table.getRowModel().rows.map((row) => (
            <tr
              key={row.id}
              {...rowAttrs?.(row.original)}
              data-testid={rowTestId}
              // A clickable row has to be reachable without a mouse.
              tabIndex={clickable ? 0 : undefined}
              onClick={clickable ? () => onRowClick?.(row.original) : undefined}
              onKeyDown={
                clickable
                  ? (event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        event.preventDefault()
                        onRowClick?.(row.original)
                      }
                    }
                  : undefined
              }
              className={clickable ? 'cursor-pointer hover:bg-primary-tint' : undefined}
              style={{ borderTop: '1px solid var(--rule)' }}
            >
              {row.getVisibleCells().map((cell) => (
                <td
                  key={cell.id}
                  className={
                    cell.column.columnDef.meta?.numeric === true
                      ? 'tabular px-4 py-[13px] text-right'
                      : 'tabular px-4 py-[13px]'
                  }
                >
                  {flexRender(cell.column.columnDef.cell, cell.getContext())}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
