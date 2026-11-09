import "server-only"
import { createHmac, timingSafeEqual } from "node:crypto"
import type { NextRequest, NextResponse } from "next/server"
import { voiceSetting } from "./settings"

// Page-proxy side of the gap ledger: kept free of the tool catalog so every navigation stays light.
/** Signature-only follow-up; Office's page proxy has no staff bearer. */
export async function followUp(
  apiUrl: string,
  workspaceId: string,
  gapId: string,
  signal: { signal: "rephrased" } | { signal: "used_ui"; route: string }
) {
  const url = new URL(
    `${apiUrl}/assistant/gaps/${encodeURIComponent(gapId)}/follow-up`
  )
  const body = JSON.stringify({ workspace_id: workspaceId, ...signal })
  await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...(await (
        await import("./bridge")
      ).bridgeHeaders("POST", url.pathname, body)),
    },
    body,
    cache: "no-store",
    redirect: "error",
    signal: AbortSignal.timeout(1000),
  })
}

export const GAP_COOKIE = "genesy_voice_gap"
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
async function mac(value: string) {
  return createHmac("sha256", await voiceSetting("ASSISTANT_BRIDGE_SECRET"))
    .update(`voice-gap\n${value}`)
    .digest("hex")
}

/** Cookie value `<workspace>.<gap>.<hmac>`: a user cannot point the follow-up at another gap. */
export async function gapCookie(workspaceId: string, gapId: string) {
  return `${workspaceId}.${gapId}.${await mac(`${workspaceId}.${gapId}`)}`
}
export async function readGapCookie(value = "") {
  const [workspaceId = "", gapId = "", signature = ""] = value.split(".")
  if (
    !UUID.test(workspaceId) ||
    !UUID.test(gapId) ||
    !/^[0-9a-f]{64}$/.test(signature)
  )
    return undefined
  const expected = Buffer.from(await mac(`${workspaceId}.${gapId}`), "hex")
  return timingSafeEqual(expected, Buffer.from(signature, "hex"))
    ? { workspaceId, gapId }
    : undefined
}

/** Static Office page segments (app router folders); anything else becomes `[id]`. */
export const ROUTE_SEGMENTS = new Set([
  "accept-invitation",
  "app-keys",
  "categories",
  "corrections",
  "design",
  "healthz",
  "invites",
  "invoices",
  "links",
  "login",
  "lots",
  "negative",
  "orders",
  "overview",
  "passport",
  "products",
  "registers",
  "sales",
  "scheduled-reports",
  "search",
  "secure",
  "settings",
  "shifts",
  "stock",
  "supply",
  "till",
  "transfers",
  "unsubscribe",
  "users",
  "vaults",
  "voice",
  "webhooks",
  "members",
  "offline",
  "sw.js",
])

/** Route pattern of an Office page; undefined for home, voice pages and route handlers. */
export function routePattern(pathname: string): string | undefined {
  const segments = pathname.split("/").filter(Boolean)
  // Home, voice pages, route handlers (/api/voice/*) and browser files (dot in a segment) are not a UI follow-up.
  if (
    !segments.length ||
    segments[0] === "voice" ||
    segments[0] === "api" ||
    segments.some((s) => s.includes("."))
  )
    return undefined
  const route = `/${segments.map((s) => (ROUTE_SEGMENTS.has(s) ? s : "[id]")).join("/")}`
  return route.length <= 80 ? route : undefined
}

/** Page proxy: after a voice gap, the next Office page the user opens is recorded as `used_ui`.
 * Fire-and-forget: never waits for, or fails on, the ledger. */
export function followGapNavigation(
  request: NextRequest,
  response: NextResponse
): NextResponse {
  const raw = request.cookies.get(GAP_COOKIE)?.value
  if (!raw || request.method !== "GET") return response
  const route = routePattern(request.nextUrl.pathname)
  if (!route) return response
  response.cookies.delete(GAP_COOKIE)
  void (async () => {
    const gap = await readGapCookie(raw)
    if (!gap) return
    const apiUrl = (await voiceSetting("API_URL")) || "http://localhost:8080/v1"
    await followUp(apiUrl, gap.workspaceId, gap.gapId, {
      signal: "used_ui",
      route,
    })
  })().catch(() => undefined)
  return response
}
