import { cspProxy } from "@genesy/security/proxy"

// shortcut: frames refused everywhere; pass { frameAncestors } here once a shop page must be embedded.
export const proxy = cspProxy()

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
