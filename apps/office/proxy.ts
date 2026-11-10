import type { NextRequest } from "next/server"
import { cspProxy } from "@genesy/security/proxy"
import { followGapNavigation } from "@/lib/voice/gap-follow-up"

// FILES_ORIGIN: the file storage host serving signed image links (product images, passport preview).
const csp = cspProxy({ imgSrc: process.env.FILES_ORIGIN, voice: true })
// Developer gap ledger: the page opened right after a voice failure (fire-and-forget).
export const proxy = (request: NextRequest) =>
  followGapNavigation(request, csp(request))

// Pages only: static files, images and the /v1 forward to the backend skip it. api/ is skipped
// too: Next cuts the body of every request the proxy sees at 10 MiB, and /api/voice/* accepts
// multipart uploads up to 10 MiB + 64 KiB.
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
