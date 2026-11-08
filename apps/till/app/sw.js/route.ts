import { serviceWorkerSource } from "@genesy/ui/lib/pwa"

// Built once per deploy, so the version (build time) changes with every release.
export const dynamic = "force-static"

// Every till screen, so the installed till opens with no network; its sale queue does the rest.
const source = serviceWorkerSource({
  precache: ["/offline", "/setup", "/login", "/sale", "/queue", "/drawer"],
  version: String(Date.now()),
})

export function GET() {
  return new Response(source, {
    headers: {
      "Content-Type": "text/javascript; charset=utf-8",
      "Cache-Control": "no-cache",
    },
  })
}
