import { cspProxy } from "@genesy/security/proxy"

// shortcut: frames refused everywhere; the landing page is never embedded.
export const proxy = cspProxy()

// Pages only: static files and images skip it.
export const config = {
  matcher: [
    {
      source: "/((?!_next/static|_next/image|favicon.ico).*)",
      missing: [
        { type: "header", key: "next-router-prefetch" },
        { type: "header", key: "purpose", value: "prefetch" },
      ],
    },
  ],
}
