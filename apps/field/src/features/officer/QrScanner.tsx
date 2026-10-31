import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import QrScannerEngine from 'qr-scanner'

/**
 * The camera, and nothing else: a thin wrapper around `qr-scanner`.
 *
 * It reports the first decoded text and stops. What that text means is the
 * screen's business — this component does not know what a voucher is, and a
 * scan of some other QR code reaches the screen exactly as a typed code would.
 *
 * Any failure to get a picture — no camera, permission refused, an insecure
 * (http) context — is one answer, `onUnavailable`, because the officer's next
 * step is the same for all of them: type the code printed under the QR.
 *
 * No drawn overlay and no animated scan line: nothing on this surface moves
 * (src/motion.test.ts).
 */
export function QrScanner({
  onResult,
  onUnavailable,
}: {
  onResult: (text: string) => void
  onUnavailable: () => void
}) {
  const { t } = useTranslation()
  const videoRef = useRef<HTMLVideoElement>(null)
  // The latest callbacks, so the camera is started once per mount rather than
  // once per parent render.
  const onResultRef = useRef(onResult)
  const onUnavailableRef = useRef(onUnavailable)
  useEffect(() => {
    onResultRef.current = onResult
    onUnavailableRef.current = onUnavailable
  })

  useEffect(() => {
    const video = videoRef.current as HTMLVideoElement
    let active = true
    let delivered = false
    let scanner: QrScannerEngine | null = null

    const unavailable = () => {
      if (active) onUnavailableRef.current()
    }

    void (async () => {
      try {
        const hasCamera = await QrScannerEngine.hasCamera()
        if (!active) return
        if (!hasCamera) return unavailable()

        const engine = new QrScannerEngine(
          video,
          (result) => {
            // One decode is one scan. The camera keeps delivering frames of the
            // same code until it is stopped, and each lookup is audited.
            if (delivered) return
            delivered = true
            engine.stop()
            onResultRef.current(result.data)
          },
          {
            returnDetailedScanResult: true,
            highlightScanRegion: false,
            highlightCodeOutline: false,
            preferredCamera: 'environment',
          },
        )
        scanner = engine
        await engine.start()
      } catch {
        unavailable()
      }
    })()

    return () => {
      active = false
      scanner?.stop()
      scanner?.destroy()
    }
  }, [])

  return (
    <video
      ref={videoRef}
      data-testid="qr-scanner-video"
      aria-label={t('redeem.scan')}
      muted
      playsInline
      className="block w-full"
      style={{
        aspectRatio: '1 / 1',
        objectFit: 'cover',
        background: 'var(--sand-2)',
        border: '1px solid var(--rule-2)',
        borderRadius: 'var(--radius-card)',
      }}
    />
  )
}
