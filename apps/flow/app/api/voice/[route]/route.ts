import { forwardVoice } from "@/lib/voice-proxy"

export const runtime = "nodejs"
export const dynamic = "force-dynamic"

// Every method lands in forwardVoice, so a wrong one gets its JSON 405 rather than Next's empty one.
async function handle(
  request: Request,
  { params }: RouteContext<"/api/voice/[route]">
) {
  return forwardVoice(request, (await params).route)
}
export {
  handle as GET,
  handle as POST,
  handle as PUT,
  handle as PATCH,
  handle as DELETE,
}
