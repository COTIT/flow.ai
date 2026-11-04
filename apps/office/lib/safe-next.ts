// A path on this site: one leading "/", never "//" or "\" (browsers read both as another host),
// and no control characters (browsers drop tabs and newlines, so "/\t/x" becomes "//x").
const sameSite = (s: string) =>
  s.startsWith("/") &&
  !s.includes("//") &&
  !s.includes("\\") &&
  // eslint-disable-next-line no-control-regex
  !/[\u0000-\u001f\u007f]/.test(s)

/**
 * Where to go after logging in: `raw` when it is a path on this site, else the overview.
 * Checked raw and decoded, so `/%2F%2Fevil.com` cannot slip through as `//evil.com`.
 */
export function safeNext(raw: string | null): string {
  if (!raw || !sameSite(raw)) return "/"
  try {
    return sameSite(decodeURIComponent(raw)) ? raw : "/"
  } catch {
    return "/"
  }
}
