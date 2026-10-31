import { render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

/**
 * A fake of the `qr-scanner` class: records what the wrapper asked of it and
 * lets a test deliver a decode as the camera would.
 */
const hasCamera = vi.fn<() => Promise<boolean>>()
const start = vi.fn<() => Promise<void>>()
const stop = vi.fn()
const destroy = vi.fn()
const instances: FakeScanner[] = []

class FakeScanner {
  static hasCamera = () => hasCamera()
  video: HTMLVideoElement
  onDecode: (result: { data: string }) => void
  options: Record<string, unknown>
  constructor(video: HTMLVideoElement, onDecode: (result: { data: string }) => void, options: Record<string, unknown>) {
    this.video = video
    this.onDecode = onDecode
    this.options = options
    instances.push(this)
  }
  start = () => start()
  stop = () => stop()
  destroy = () => destroy()
}

vi.mock('qr-scanner', () => ({ default: FakeScanner }))

const { QrScanner } = await import('@/features/officer/QrScanner')
await import('@/i18n')

beforeEach(() => {
  instances.length = 0
  hasCamera.mockReset().mockResolvedValue(true)
  start.mockReset().mockResolvedValue(undefined)
  stop.mockReset()
  destroy.mockReset()
})

describe('QrScanner', () => {
  test('starts the rear camera on a video element, with no overlay drawn', async () => {
    render(<QrScanner onResult={vi.fn()} onUnavailable={vi.fn()} />)

    await waitFor(() => expect(start).toHaveBeenCalledTimes(1))
    expect(instances).toHaveLength(1)
    expect(instances[0].video).toBe(screen.getByTestId('qr-scanner-video'))
    expect(instances[0].options).toEqual({
      returnDetailedScanResult: true,
      highlightScanRegion: false,
      highlightCodeOutline: false,
      preferredCamera: 'environment',
    })
  })

  // One scan is one lookup, and every lookup writes an audit event — so the
  // second frame of the same QR code must not become a second scan.
  test('delivers the first decode once and stops the camera', async () => {
    const onResult = vi.fn()
    render(<QrScanner onResult={onResult} onUnavailable={vi.fn()} />)
    await waitFor(() => expect(instances).toHaveLength(1))

    instances[0].onDecode({ data: 'R360V:K7QXM2PA9D' })
    instances[0].onDecode({ data: 'R360V:K7QXM2PA9D' })

    expect(onResult).toHaveBeenCalledTimes(1)
    expect(onResult).toHaveBeenCalledWith('R360V:K7QXM2PA9D')
    expect(stop).toHaveBeenCalled()
  })

  test('stops and destroys the scanner on unmount', async () => {
    const view = render(<QrScanner onResult={vi.fn()} onUnavailable={vi.fn()} />)
    await waitFor(() => expect(start).toHaveBeenCalled())

    view.unmount()
    expect(stop).toHaveBeenCalled()
    expect(destroy).toHaveBeenCalledTimes(1)
  })

  test('no camera is "unavailable", and no scanner is built', async () => {
    hasCamera.mockResolvedValue(false)
    const onUnavailable = vi.fn()
    render(<QrScanner onResult={vi.fn()} onUnavailable={onUnavailable} />)

    await waitFor(() => expect(onUnavailable).toHaveBeenCalledTimes(1))
    expect(instances).toHaveLength(0)
  })

  // Permission denied and an insecure (http) context both surface as a failed
  // start; the officer types the code instead.
  test('a refused camera is "unavailable"', async () => {
    start.mockRejectedValue(new Error('Camera not found.'))
    const onUnavailable = vi.fn()
    render(<QrScanner onResult={vi.fn()} onUnavailable={onUnavailable} />)

    await waitFor(() => expect(onUnavailable).toHaveBeenCalledTimes(1))
  })

  test('a failing camera check is "unavailable"', async () => {
    hasCamera.mockRejectedValue(new Error('navigator.mediaDevices is undefined'))
    const onUnavailable = vi.fn()
    render(<QrScanner onResult={vi.fn()} onUnavailable={onUnavailable} />)

    await waitFor(() => expect(onUnavailable).toHaveBeenCalledTimes(1))
  })

  // Closing the camera before it answered must not report it unavailable to a
  // screen that has moved on, nor start a scanner nobody will stop.
  test('unmounting before the camera answers starts nothing and reports nothing', async () => {
    let answer: (value: boolean) => void = () => {}
    hasCamera.mockReturnValue(new Promise((resolve) => (answer = resolve)))
    const onUnavailable = vi.fn()
    const view = render(<QrScanner onResult={vi.fn()} onUnavailable={onUnavailable} />)

    view.unmount()
    answer(false)
    await Promise.resolve()
    await Promise.resolve()

    expect(instances).toHaveLength(0)
    expect(onUnavailable).not.toHaveBeenCalled()
  })

  test('a start that fails after unmount reports nothing', async () => {
    let fail: (error: Error) => void = () => {}
    start.mockReturnValue(new Promise((_, reject) => (fail = reject)))
    const onUnavailable = vi.fn()
    const view = render(<QrScanner onResult={vi.fn()} onUnavailable={onUnavailable} />)
    await waitFor(() => expect(start).toHaveBeenCalled())

    view.unmount()
    fail(new Error('stopped'))
    await Promise.resolve()
    await Promise.resolve()

    expect(onUnavailable).not.toHaveBeenCalled()
    expect(destroy).toHaveBeenCalledTimes(1)
  })
})
