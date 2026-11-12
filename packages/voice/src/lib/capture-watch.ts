type Hub = {
  original: MediaDevices["getUserMedia"]
  wrapped: MediaDevices["getUserMedia"]
  subscribers: Set<(track: MediaStreamTrack) => void>
}
// On the MediaDevices object itself, so two providers or a hot reload share one wrapper.
const HUB = Symbol.for("genesy.voice.captureHub")
type WithHub = MediaDevices & { [HUB]?: Hub }

/** One getUserMedia wrapper per MediaDevices: it only reports audio tracks, never alters a call. */
function subscribe(media: WithHub, onTrack: (track: MediaStreamTrack) => void) {
  let hub = media[HUB]
  if (!hub) {
    const original = media.getUserMedia
    const subscribers = new Set<(track: MediaStreamTrack) => void>()
    const wrapped: MediaDevices["getUserMedia"] = async (constraints) => {
      // Same arguments in, same stream out, same error thrown.
      const stream = await original.call(media, constraints)
      for (const track of stream.getAudioTracks())
        for (const notify of subscribers) notify(track)
      return stream
    }
    hub = { original, wrapped, subscribers }
    media[HUB] = hub
    media.getUserMedia = wrapped
  }
  const current = hub
  current.subscribers.add(onTrack)
  return () => {
    current.subscribers.delete(onTrack)
    if (current.subscribers.size || media[HUB] !== current) return
    delete media[HUB]
    if (media.getUserMedia === current.wrapped)
      media.getUserMedia = current.original
  }
}

/**
 * Calls `onLost` when the microphone is taken away mid-capture: a capture track ends (unplugged,
 * the OS or browser revoked it) or its input leaves the device list. Our own `stop()` never fires
 * "ended". Returns the cleanup: every listener removed, the wrapper gone with its last watcher.
 *
 * shortcut: the voice SDK keeps its capture stream private, so its tracks are seen by wrapping
 * getUserMedia; switch to the SDK's own input stream or track events once it exposes them.
 */
export function watchCapture(media: MediaDevices, onLost: () => void) {
  const tracks = new Set<MediaStreamTrack>()
  const ended = (event: Event) => {
    forget(event.target as MediaStreamTrack)
    onLost()
  }
  function forget(track: MediaStreamTrack) {
    track.removeEventListener("ended", ended)
    tracks.delete(track)
  }
  const unsubscribe = subscribe(media, (track) => {
    for (const old of tracks) if (old.readyState === "ended") forget(old)
    tracks.add(track)
    track.addEventListener("ended", ended)
  })
  const deviceChange = async () => {
    for (const old of tracks) if (old.readyState === "ended") forget(old)
    if (!tracks.size) return
    const inputs = new Set(
      (await media.enumerateDevices())
        .filter((device) => device.kind === "audioinput")
        .map((device) => device.deviceId)
    )
    const gone = [...tracks].some((track) => {
      const id = track.getSettings().deviceId
      return !inputs.size || (id !== undefined && !inputs.has(id))
    })
    if (gone) onLost()
  }
  media.addEventListener("devicechange", deviceChange)
  return () => {
    unsubscribe()
    media.removeEventListener("devicechange", deviceChange)
    for (const track of [...tracks]) forget(track)
  }
}
