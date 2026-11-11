import { describe, expect, it, vi } from "vitest"

import { watchCapture } from "./capture-watch"

/** A microphone track the test can end (unplugged / revoked) or stop (our own close). */
function fakeTrack(deviceId = "mic-1") {
  const track = new EventTarget() as EventTarget & {
    readyState: string
    getSettings: () => { deviceId: string }
    stop: () => void
  }
  track.readyState = "live"
  track.getSettings = () => ({ deviceId })
  track.stop = () => void (track.readyState = "ended") // stop() never fires "ended"
  return track
}

function fakeMedia(devices: { kind: string; deviceId: string }[] = []) {
  const media = new EventTarget() as EventTarget & {
    getUserMedia: (c?: unknown) => Promise<unknown>
    enumerateDevices: () => Promise<unknown[]>
    tracks: ReturnType<typeof fakeTrack>[]
    devices: typeof devices
  }
  media.tracks = []
  media.devices = devices
  media.getUserMedia = vi.fn(async () => {
    const track = fakeTrack()
    media.tracks.push(track)
    return { getAudioTracks: () => [track] }
  })
  media.enumerateDevices = async () => media.devices
  return media
}

const settle = () => new Promise((r) => setTimeout(r, 0))

describe("watchCapture", () => {
  it("a capture track that ends (unplugged, revoked) is a lost microphone", async () => {
    const media = fakeMedia()
    const lost = vi.fn()
    watchCapture(media as unknown as MediaDevices, lost)
    await media.getUserMedia({ audio: true })
    media.tracks[0]!.dispatchEvent(new Event("ended"))
    expect(lost).toHaveBeenCalledOnce()
  })

  it("our own stop() is not a loss", async () => {
    const media = fakeMedia([{ kind: "audioinput", deviceId: "mic-1" }])
    const lost = vi.fn()
    watchCapture(media as unknown as MediaDevices, lost)
    await media.getUserMedia({ audio: true })
    media.tracks[0]!.stop()
    media.devices = []
    media.dispatchEvent(new Event("devicechange"))
    await settle()
    expect(lost).not.toHaveBeenCalled()
  })

  it("the capturing input leaving the device list is a lost microphone", async () => {
    const media = fakeMedia([
      { kind: "audioinput", deviceId: "mic-1" },
      { kind: "audioinput", deviceId: "mic-2" },
    ])
    const lost = vi.fn()
    watchCapture(media as unknown as MediaDevices, lost)
    await media.getUserMedia({ audio: true })
    media.devices = [{ kind: "audioinput", deviceId: "mic-2" }]
    media.dispatchEvent(new Event("devicechange"))
    await settle()
    expect(lost).toHaveBeenCalledOnce()
  })

  it("another device coming or going is not a loss", async () => {
    const media = fakeMedia([{ kind: "audioinput", deviceId: "mic-1" }])
    const lost = vi.fn()
    watchCapture(media as unknown as MediaDevices, lost)
    await media.getUserMedia({ audio: true })
    media.devices = [
      { kind: "audioinput", deviceId: "mic-1" },
      { kind: "audiooutput", deviceId: "speaker" },
    ]
    media.dispatchEvent(new Event("devicechange"))
    await settle()
    expect(lost).not.toHaveBeenCalled()
  })

  it("cleanup restores getUserMedia and leaves no listeners behind", async () => {
    const media = fakeMedia([])
    const original = media.getUserMedia
    const lost = vi.fn()
    const stop = watchCapture(media as unknown as MediaDevices, lost)
    expect(media.getUserMedia).not.toBe(original)
    await media.getUserMedia({ audio: true })
    stop()
    expect(media.getUserMedia).toBe(original)
    media.tracks[0]!.dispatchEvent(new Event("ended"))
    media.dispatchEvent(new Event("devicechange"))
    await settle()
    expect(lost).not.toHaveBeenCalled()
  })

  it("two watchers share one wrapper; the last one out restores getUserMedia", async () => {
    const media = fakeMedia([])
    const original = media.getUserMedia
    const first = vi.fn()
    const second = vi.fn()
    const stopFirst = watchCapture(media as unknown as MediaDevices, first)
    const wrapper = media.getUserMedia
    const stopSecond = watchCapture(media as unknown as MediaDevices, second)
    expect(media.getUserMedia).toBe(wrapper) // not wrapped twice
    await media.getUserMedia({ audio: true })
    media.tracks[0]!.dispatchEvent(new Event("ended"))
    expect(first).toHaveBeenCalledOnce()
    expect(second).toHaveBeenCalledOnce()
    stopFirst()
    expect(media.getUserMedia).toBe(wrapper)
    stopSecond()
    expect(media.getUserMedia).toBe(original)
  })

  it("leaves a getUserMedia someone else installed later in place", () => {
    const media = fakeMedia([])
    const stop = watchCapture(media as unknown as MediaDevices, vi.fn())
    const later = vi.fn()
    media.getUserMedia = later
    stop()
    expect(media.getUserMedia).toBe(later)
  })

  it("passes the same arguments, stream and error through", async () => {
    const media = fakeMedia([])
    const stream = { getAudioTracks: () => [] }
    const refused = new DOMException("denied", "NotAllowedError")
    const original = vi
      .fn()
      .mockResolvedValueOnce(stream)
      .mockRejectedValueOnce(refused)
    media.getUserMedia = original
    watchCapture(media as unknown as MediaDevices, vi.fn())
    const constraints = { audio: { deviceId: "mic-2" } }
    await expect(media.getUserMedia(constraints)).resolves.toBe(stream)
    expect(original).toHaveBeenCalledWith(constraints)
    await expect(media.getUserMedia(constraints)).rejects.toBe(refused)
  })

  it("an ended track is dropped, so it reports once", async () => {
    const media = fakeMedia([])
    const lost = vi.fn()
    watchCapture(media as unknown as MediaDevices, lost)
    await media.getUserMedia({ audio: true })
    const track = media.tracks[0]!
    track.dispatchEvent(new Event("ended"))
    track.dispatchEvent(new Event("ended"))
    expect(lost).toHaveBeenCalledOnce()
  })
})
