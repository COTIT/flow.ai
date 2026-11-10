import type { NextRequest } from "next/server"

import { cspProxy } from "@genesy/security/proxy"

// Same CSP as Office: fresh nonce, no framing, ElevenLabs connect for the assistant.
const page = cspProxy({ voice: true })
// The storefront preview alone may sit in a frame, and only on Flow's own pages. It talks to
// no voice server, so its policy drops those origins. Exact path: anything else stays 'none'.
const preview = cspProxy({ frameAncestors: "'self'" })

export function proxy(request: NextRequest) {
  return request.nextUrl.pathname === "/storefront/preview"
    ? preview(request)
    : page(request)
}

// Pages only. api/ is skipped too: Next buffers (and past 10 MB truncates) the body of every
// request the proxy sees, and /api/voice/* streams uploads to Office.
export const config = {
  matcher: [
    {
      source: "/((?!v1/|api/|_next/static|_next/image|favicon.ico).*)",
      missing: [
        { type: "header", key: "next-router-prefetch" },
        { type: "header", key: "purpose", value: "prefetch" },
      ],
    },
  ],
}
