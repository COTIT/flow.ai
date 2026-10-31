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
  }: { dev?: boolean; frameAncestors?: string } = {}
): string {
  return [
    "default-src 'self'",
    `script-src 'self' 'nonce-${nonce}' 'strict-dynamic'${dev ? " 'unsafe-eval'" : ""}`,
    // Inline styles stay allowed: style attributes can't run code, and components set them.
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' blob: data:",
    "font-src 'self'",
    `connect-src 'self'${dev ? " ws:" : ""}`,
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    `frame-ancestors ${frameAncestors}`,
  ].join("; ")
}

/** Headers for every response, set in each app's next.config `headers()`. */
export const securityHeaders = [
  { key: "X-Content-Type-Options", value: "nosniff" },
  { key: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
]
