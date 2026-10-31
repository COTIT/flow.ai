import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, test, vi } from 'vitest'

import '@/i18n'
import { Button } from '@/components/ui/button'
import { BreadcrumbItem, BreadcrumbLink, Breadcrumbs } from '@/components/ui/breadcrumb'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to, ...props }: { children: React.ReactNode; to?: string } & Record<string, unknown>) => (
    <a href={to ?? '#'} {...props}>{children}</a>
  ),
}))

describe('shared shadcn primitives', () => {
  test('Button preserves native semantics and exposes its visual variant', () => {
    render(<Button variant="secondary">Save changes</Button>)

    const button = screen.getByRole('button', { name: 'Save changes' })
    expect(button).toHaveAttribute('type', 'button')
    expect(button.className).toContain('bg-paper')
    expect(button.className).toContain('min-h-11')
  })

  test('Select opens, selects an option, and supports keyboard selection', async () => {
    const onValueChange = vi.fn()
    const user = userEvent.setup()
    render(
      <Select onValueChange={onValueChange}>
        <SelectTrigger aria-label="Village"><SelectValue placeholder="Choose a village" /></SelectTrigger>
        <SelectContent>
          <SelectItem value="ilundo">Ilundo</SelectItem>
          <SelectItem value="mgama">Mgama</SelectItem>
        </SelectContent>
      </Select>,
    )

    const trigger = screen.getByRole('combobox', { name: 'Village' })
    await user.click(trigger)
    await waitFor(() => expect(screen.getByRole('option', { name: 'Ilundo' })).toBeVisible())
    await user.keyboard('{ArrowDown}{Enter}')

    expect(onValueChange).toHaveBeenCalledWith('ilundo', expect.anything())
    expect(trigger).toHaveTextContent('ilundo')
  })

  test('Breadcrumbs expose navigation and the current page to assistive technology', () => {
    render(
      <Breadcrumbs>
        <BreadcrumbItem><BreadcrumbLink href="/ops">Operations</BreadcrumbLink></BreadcrumbItem>
        <BreadcrumbItem current>Requests</BreadcrumbItem>
      </Breadcrumbs>,
    )

    expect(screen.getByRole('navigation', { name: 'Breadcrumb' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Operations' })).toHaveAttribute('href', '/ops')
    expect(screen.getByText('Requests').closest('li')).toHaveAttribute('aria-current', 'page')
  })
})
