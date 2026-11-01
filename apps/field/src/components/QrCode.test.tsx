import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'

import { QrCode } from '@/components/QrCode'

describe('QrCode', () => {
  test('draws the payload as an SVG, never an image file', () => {
    render(<QrCode value="R360V:K7QXM2PA9D" label="QR code for voucher K7QXM-2PA9D" />)
    const svg = screen.getByRole('img', { name: 'QR code for voucher K7QXM-2PA9D' })
    expect(svg.tagName.toLowerCase()).toBe('svg')
    expect(svg.querySelectorAll('rect').length).toBeGreaterThan(50)
  })

  test('the same payload always draws the same code', () => {
    const { container: a } = render(<QrCode value="R360V:K7QXM2PA9D" label="a" />)
    const { container: b } = render(<QrCode value="R360V:K7QXM2PA9D" label="b" />)
    expect(a.querySelector('svg')?.innerHTML).toBe(b.querySelector('svg')?.innerHTML)
  })

  test('a different payload draws a different code', () => {
    const { container: a } = render(<QrCode value="R360V:K7QXM2PA9D" label="a" />)
    const { container: b } = render(<QrCode value="R360V:ABCDE12345" label="b" />)
    expect(a.querySelector('svg')?.innerHTML).not.toBe(b.querySelector('svg')?.innerHTML)
  })
})
