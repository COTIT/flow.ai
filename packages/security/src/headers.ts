/**
 * Content-Security-Policy for one response. Scripts run only with this response's
 * nonce (Next adds it to its own tags); `strict-dynamic` lets those load the rest.
 * The login token sits in localStorage, so blocking injected script is the main guard.
 */
export function contentSecurityPolicy(
  nonce: string,
  {
    dev = false,
    frameAncestors = "'none'",
    imgSrc,
    voice = false,
  }: {
    dev?: boolean
    frameAncestors?: string
    imgSrc?: string
    voice?: boolean
  } = {}
): string {
  const files = exactOrigin(imgSrc)
  return [
    "default-src 'self'",
    `script-src 'self' 'nonce-${nonce}' 'strict-dynamic'${dev ? " 'unsafe-eval'" : ""}`,
    // Inline styles stay allowed: style attributes can't run code, and components set them.
    "style-src 'self' 'unsafe-inline'",
    `img-src 'self' blob: data:${files ? ` ${files}` : ""}`,
    "font-src 'self'",
    `connect-src 'self'${voice ? " https://api.elevenlabs.io wss://api.elevenlabs.io" : ""}${dev ? " ws:" : ""}`,
    // The app's own service worker (/sw.js); 'strict-dynamic' above would otherwise block it.
    `worker-src 'self'${voice ? " blob:" : ""}`,
    ...(voice ? ["media-src 'self' blob:"] : []),
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    `frame-ancestors ${frameAncestors}`,
  ].join("; ")
}

/**
 * `value` if it is exactly one http(s) origin (scheme, host, optional port; no path,
 * wildcard or second source), else undefined, so a bad setting can only narrow the CSP.
 */
function exactOrigin(value: string | undefined): string | undefined {
  if (!value || value.includes("*")) return undefined
  try {
    const url = new URL(value)
    const ok = url.protocol === "https:" || url.protocol === "http:"
    return ok && url.origin === value.replace(/\/$/, "")
      ? url.origin
      : undefined
  } catch {
    return undefined
  }
}

/** Headers for every response, set in each app's next.config `headers()`. */
export const securityHeaders = [
  { key: "X-Content-Type-Options", value: "nosniff" },
  { key: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
]
