/**
 * Typed amount → integer minor units, by string, no floats: "12.5" with 2 decimals → 1250.
 * null when it isn't a plain positive amount with at most `decimals` places.
 */
export function toMinor(text: string, decimals: number): number | null {
  const m = /^(\d+)(?:[.,](\d*))?$/.exec(text.trim())
  if (!m || (m[2] ?? "").length > decimals) return null
  const minor = BigInt(m[1]! + (m[2] ?? "").padEnd(decimals, "0"))
  return minor > 0n && minor <= BigInt(Number.MAX_SAFE_INTEGER)
    ? Number(minor)
    : null
}
