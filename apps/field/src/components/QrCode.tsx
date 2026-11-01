import { useMemo } from 'react'
import { encode } from 'uqr'

/**
 * A QR code drawn as SVG modules — never an image file (the depth guard
 * forbids raster assets) and never innerHTML. Dark modules are ink on a white
 * quiet zone in both themes: a scanner needs the contrast, not the palette.
 */
export function QrCode({ value, label, size = 224 }: { value: string; label: string; size?: number }) {
  const qr = useMemo(() => encode(value, { ecc: 'M', border: 2 }), [value])

  return (
    <svg
      role="img"
      aria-label={label}
      width={size}
      height={size}
      viewBox={`0 0 ${qr.size} ${qr.size}`}
      shapeRendering="crispEdges"
      style={{ background: '#ffffff', display: 'block' }}
    >
      {qr.data.flatMap((row, y) =>
        row.map((dark, x) =>
          dark ? <rect key={`${x}-${y}`} x={x} y={y} width={1} height={1} fill="#000000" /> : null,
        ),
      )}
    </svg>
  )
}
