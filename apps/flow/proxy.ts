import type { NextRequest } from "next/server"

import { cspProxy } from "@genesy/security/proxy"

// FILES_ORIGIN: the file storage host serving signed product images (set by the uploads overlay).
const imgSrc = process.env.FILES_ORIGIN
// Same CSP as Office: fresh nonce, no framing, ElevenLabs connect for the assistant.
const page = cspProxy({ imgSrc, voice: true })
// The storefront preview alone may sit in a frame, and only on Flow's own pages. It talks to
// no voice server, so its policy drops those origins. Exact path: anything else stays 'none'.
const preview = cspProxy({ imgSrc, frameAncestors: "'self'" })

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
