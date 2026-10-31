/**
 * The Ruaha Energy lockup, and where the product name sits next to it.
 *
 * Ruaha 360 is a programme surface, not a second brand: the supplied lockup
 * stays whole, and `360` follows it after a hairline divider in the interface's
 * own type. Never re-coloured, never on a photograph, never below 20px — the
 * wordmark stops being legible there.
 *
 * 26px in the desktop header, 23px on field surfaces, 34px on the login card.
 *
 * An `<img>` rather than an inline SVG: it is 3.5 kB, costs one request the PWA
 * precaches, and scales at any density. The supplied file carries its own
 * fills — see docs/design/ORIGIN.md for why the copies in the design bundle do
 * not.
 */
export function BrandLockup({ height = 26 }: { height?: number }) {
  if (height < 20) {
    throw new Error(`The Ruaha lockup is illegible below 20px; asked for ${height}px`)
  }

  return (
    <span className="inline-flex items-center" style={{ gap: Math.round(height * 0.42) }}>
      <img
        src="/ruaha-logo.svg"
        alt="Ruaha Energy"
        style={{ height, width: 'auto', display: 'block' }}
      />
      <span
        aria-hidden
        style={{ width: 1, height: Math.round(height * 0.77), background: 'var(--rule-2)' }}
      />
      <span
        style={{
          fontSize: Math.max(14, Math.round(height * 0.58)),
          fontWeight: 600,
          letterSpacing: '.02em',
          color: 'var(--ink-2)',
        }}
      >
        360
      </span>
    </span>
  )
}
