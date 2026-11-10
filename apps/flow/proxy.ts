import { cspProxy } from "@genesy/security/proxy"

// Same CSP as Office: fresh nonce, no framing, ElevenLabs connect for the assistant.
export const proxy = cspProxy({ voice: true })

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
