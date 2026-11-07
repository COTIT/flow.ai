import { assistantReadiness } from "@/lib/voice/readiness"

export const runtime = "nodejs"
export const dynamic = "force-dynamic"
export async function GET() {
  const result = await assistantReadiness()
  return Response.json(result, {
    status: result.ready ? 200 : 503,
    headers: { "Cache-Control": "no-store" },
  })
}
