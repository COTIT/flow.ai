import { cspProxy } from "@genesy/security/proxy"

// FILES_ORIGIN: the file storage host serving signed image links (product images, passport preview).
export const proxy = cspProxy({ imgSrc: process.env.FILES_ORIGIN, voice: true })

// Pages only: static files, images and the /v1 forward to the backend skip it.
export const config = {
  matcher: [
    {
      source: "/((?!v1/|_next/static|_next/image|favicon.ico).*)",
      missing: [
        { type: "header", key: "next-router-prefetch" },
        { type: "header", key: "purpose", value: "prefetch" },
      ],
    },
  ],
}
