// Liveness only: no auth, no upstream call (the container healthcheck).
export const dynamic = "force-dynamic"
export function GET() {
  return Response.json(
    { status: "ok" },
    { headers: { "Cache-Control": "no-store" } }
  )
}
