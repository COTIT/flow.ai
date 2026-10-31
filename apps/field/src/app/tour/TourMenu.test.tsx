import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, test, vi } from 'vitest'

import { TourMenu } from '@/app/tour/TourMenu'

await import('@/i18n')

const CHAPTERS = [
  { id: 'welcome', titleKey: 'tour.officer.chapter.welcome' },
  { id: 'register', titleKey: 'tour.officer.chapter.register' },
  { id: 'redeem', titleKey: 'tour.officer.chapter.redeem' },
]

/**
 * The tour is a set of chapters, and the person running a demo needs to jump to
 * the module in front of them rather than click through everything before it.
 * So the way back to the tour is a menu: the whole tour, or one chapter.
 */
describe('choosing a part of the tour', () => {
  test('offers the whole tour and then every chapter, by name', () => {
    render(<TourMenu chapters={CHAPTERS} onPick={vi.fn()} onClose={vi.fn()} />)

    expect(screen.getByRole('dialog', { name: 'Choose a part of the tour' })).toBeInTheDocument()
    expect(screen.getByTestId('tour-chapter-all')).toHaveTextContent('Play the whole tour')
    expect(screen.getByTestId('tour-chapter-register')).toHaveTextContent('Registering a farmer')
    expect(screen.getByTestId('tour-chapter-redeem')).toHaveTextContent('Redeeming a voucher')
  })

  test('picking a chapter plays that chapter', async () => {
    const onPick = vi.fn()
    render(<TourMenu chapters={CHAPTERS} onPick={onPick} onClose={vi.fn()} />)

    await userEvent.click(screen.getByTestId('tour-chapter-register'))
    expect(onPick).toHaveBeenCalledWith('register')
  })

  test('picking the whole tour plays all of it', async () => {
    const onPick = vi.fn()
    render(<TourMenu chapters={CHAPTERS} onPick={onPick} onClose={vi.fn()} />)

    await userEvent.click(screen.getByTestId('tour-chapter-all'))
    expect(onPick).toHaveBeenCalledWith('all')
  })

  test('can be closed without choosing, three ways', async () => {
    const onClose = vi.fn()
    render(<TourMenu chapters={CHAPTERS} onPick={vi.fn()} onClose={onClose} />)

    await userEvent.click(screen.getByTestId('tour-menu-close'))
    await userEvent.keyboard('{Escape}')
    await userEvent.click(screen.getByTestId('tour-menu-backdrop'))
    expect(onClose).toHaveBeenCalledTimes(3)
  })

  test('a click inside the menu is not a click on the backdrop', async () => {
    const onClose = vi.fn()
    render(<TourMenu chapters={CHAPTERS} onPick={vi.fn()} onClose={onClose} />)

    await userEvent.click(screen.getByRole('dialog'))
    expect(onClose).not.toHaveBeenCalled()
  })

  test('puts the keyboard on the first choice', () => {
    render(<TourMenu chapters={CHAPTERS} onPick={vi.fn()} onClose={vi.fn()} />)
    expect(screen.getByTestId('tour-chapter-all')).toHaveFocus()
  })
})
