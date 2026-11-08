import { serviceWorkerSource } from "@genesy/ui/lib/pwa"

// Built once per deploy, so the version (build time) changes with every release.
export const dynamic = "force-static"

const source = serviceWorkerSource({ precache: ["/offline"], version: String(Date.now()) })

export function GET() {
  return new Response(source, {
    headers: {
      "Content-Type": "text/javascript; charset=utf-8",
      "Cache-Control": "no-cache",
    },
  })
}
